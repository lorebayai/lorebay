use std::slice::from_ref;

use sea_orm::{ActiveModelTrait, DatabaseTransaction, IntoActiveModel};
use serde::Serialize;
use typed_builder::TypedBuilder;

use crate::entity::{space, space_person_request as person_request};
use crate::repository::{KnowledgeRepository, PersonRequestRepository, SpaceRepository};
use crate::{Caller, LibraryState, OperationHandle, Write, WriteOperation};

use super::id::primary_key;
use super::key::parse_key;
use super::person_request_list::{PersonRequestListItem, named_items};
use super::space_access::{READ_ROLES, WRITE_ROLES, check_space};
use super::task_access::{TaskAction, check_task};
use super::time::now_millis;
use super::{KnowledgeError, PersonRequestContent, PersonRequestError, PersonRequestSource};

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct PersonRequest {
    #[serde(flatten)]
    pub details: PersonRequestListItem,
    pub new_content: Option<String>,
    pub from_task_key: Option<String>,
    pub base_version: Option<i64>,
    pub decided_by: Option<String>,
    pub decided_at: Option<i64>,
    pub reject_reason: Option<String>,
    pub base_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct SubmittedPersonRequest {
    #[serde(flatten)]
    pub person_request: PersonRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_title: Option<String>,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct PersonRequestSpace {
    pub key: String,
    pub name: String,
}

impl From<space::Model> for PersonRequestSpace {
    fn from(space: space::Model) -> Self {
        Self {
            key: super::key::format_key("SPACE", space.number),
            name: space.name,
        }
    }
}

pub async fn submit_person_request(
    write: &mut Write<'_>,
    caller: &Caller,
    source: PersonRequestSource<'_>,
    content: PersonRequestContent,
) -> Result<SubmittedPersonRequest, PersonRequestError> {
    let (request, _) = submit_request(write, caller, source, content).await?;
    Ok(request)
}

pub async fn submit_person_request_with_space(
    write: &mut Write<'_>,
    caller: &Caller,
    source: PersonRequestSource<'_>,
    content: PersonRequestContent,
) -> Result<(SubmittedPersonRequest, PersonRequestSpace), PersonRequestError> {
    let (request, space_id) = submit_request(write, caller, source, content).await?;
    let space = SpaceRepository::builder()
        .connection(write.transaction())
        .build()
        .by_id(&space_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Request space is missing"))?;
    Ok((request, space.into()))
}

pub async fn read_person_request(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    key: &str,
) -> Result<PersonRequest, PersonRequestError> {
    let (request, _) = read_person_request_with_space(state, caller, space, key).await?;
    Ok(request)
}

pub async fn read_person_request_with_space(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    key: &str,
) -> Result<(PersonRequest, PersonRequestSpace), PersonRequestError> {
    state
        .run_read(async |handle| {
            let space = check_space(state, handle, caller, space, READ_ROLES).await?;
            let number = parse_key("RQ", key).ok_or(PersonRequestError::NotFound)?;
            let request = person_request_result(state, handle, &space.space.id, number).await?;
            Ok((request, space.space.into()))
        })
        .await
}

async fn submit_request(
    write: &mut Write<'_>,
    caller: &Caller,
    source: PersonRequestSource<'_>,
    content: PersonRequestContent,
) -> Result<(SubmittedPersonRequest, String), PersonRequestError> {
    write.begin_operation(Some(caller))?;
    let wait = matches!(source, PersonRequestSource::WaitingTask(_));
    let (space_id, task) = submission_space(write, caller, source).await?;
    let mut model = proposed_model(write.transaction(), caller, &space_id, content).await?;
    model.from_task_id = task.as_ref().map(|(id, _)| id.clone());
    model.number = SpaceRepository::builder()
        .connection(write.transaction())
        .build()
        .take_number(&space_id, space::Column::NextPersonRequestNumber)
        .await?;
    let number = model.number;
    if wait && let Some((id, _)) = &task {
        wait_for_request(write, caller, id, &model.id, number).await?;
    }
    PersonRequestRepository::builder()
        .connection(write.transaction())
        .build()
        .insert(model.into_active_model())
        .await?;
    let result = person_request_result(write.state(), write.handle(), &space_id, number).await?;
    write.record(WriteOperation::SubmitPersonRequest, Some(space_id.clone()));
    Ok((
        SubmittedPersonRequest {
            person_request: result,
            task_title: task.map(|(_, title)| title),
        },
        space_id,
    ))
}

pub(super) async fn person_request_result(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    space: &str,
    number: i64,
) -> Result<PersonRequest, PersonRequestError> {
    let row = PersonRequestRepository::builder()
        .connection(handle.transaction())
        .build()
        .read(space, number)
        .await?
        .ok_or(PersonRequestError::NotFound)?;
    let names = state
        .names()
        .names(
            handle,
            &[
                Some(row.model.requested_by.clone()),
                row.model.decided_by.clone(),
                row.model.revised_by.clone(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
        )
        .await?;
    let details = named_items(from_ref(&row), &names, handle.transaction())
        .await?
        .pop()
        .ok_or_else(|| anyhow::anyhow!("Request is missing"))?;
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
    Ok(PersonRequest {
        details,
        new_content: row.model.new_content,
        from_task_key: row
            .task_number
            .map(|number| super::key::format_key("TK", number)),
        base_version: row.model.base_version,
        decided_by,
        decided_at: row.model.decided_at,
        reject_reason: row.model.reject_reason,
        base_name: row.model.base_name,
    })
}

async fn submission_space(
    write: &Write<'_>,
    caller: &Caller,
    source: PersonRequestSource<'_>,
) -> Result<(String, Option<(String, String)>), PersonRequestError> {
    match source {
        PersonRequestSource::Space(key) => {
            let space =
                check_space(write.state(), write.handle(), caller, key, WRITE_ROLES).await?;
            caller.person()?;
            Ok((space.space.id, None))
        }
        PersonRequestSource::Task(target) | PersonRequestSource::WaitingTask(target) => {
            caller.agent()?;
            let task = check_task(
                write.state(),
                write.handle(),
                caller,
                target.check(TaskAction::SubmitPersonRequest),
            )
            .await?;
            Ok((task.space_id, Some((task.id, task.title))))
        }
    }
}

async fn proposed_model(
    transaction: &DatabaseTransaction,
    caller: &Caller,
    space: &str,
    content: PersonRequestContent,
) -> Result<person_request::Model, PersonRequestError> {
    let (action, target, name, summary, content) = match content {
        PersonRequestContent::Create(input) => (
            "create",
            None,
            Some(input.name),
            Some(input.summary),
            Some(input.content),
        ),
        PersonRequestContent::Change(target, changes) => (
            "change",
            Some(target),
            changes.name,
            changes.summary,
            changes.content,
        ),
        PersonRequestContent::Delete(target) => ("delete", Some(target), None, None, None),
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
    Ok(person_request::Model {
        id: primary_key(),
        space_id: space.to_owned(),
        number: 0,
        action: action.to_owned(),
        knowledge_id: entry.as_ref().map(|entry| entry.id.clone()),
        base_version: entry.as_ref().map(|entry| entry.latest_version),
        base_name: entry.map(|entry| entry.name),
        revision: 1,
        reviser_agent_name: None,
        revised_by: None,
        revised_at: None,
        new_name: name,
        new_summary: summary,
        new_content: content,
        requested_by: caller.person_id.clone(),
        requester_agent_name: caller.agent_name.clone(),
        from_task_id: None,
        status: "pending".to_owned(),
        decided_by: None,
        decided_at: None,
        reject_reason: None,
        created_at: now_millis()?,
    })
}

async fn wait_for_request(
    write: &Write<'_>,
    caller: &Caller,
    task_id: &str,
    request_id: &str,
    number: i64,
) -> Result<(), PersonRequestError> {
    let repository = crate::repository::TaskRepository::builder()
        .connection(write.transaction())
        .build();
    let mut task = repository
        .lock_by_id(task_id)
        .await?
        .ok_or(super::TaskAccessError::NotFound)?;
    if task.state != "working" {
        return Err(super::TaskAccessError::StateInvalid(task.state).into());
    }
    task.state = "waiting".to_owned();
    super::task_state::record_state_change(&mut task, caller, now_millis()?);
    task.state_reason = Some(format!("Waiting on request RQ-{number}"));
    task.waiting_request_id = Some(request_id.to_owned());
    repository
        .update(task.into_active_model().reset_all())
        .await?;
    Ok(())
}
