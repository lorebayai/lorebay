use std::collections::BTreeSet;

use futures_util::future::try_join_all;
use lorebay_lib_core::{
    Caller, KnowledgeChanges, KnowledgeEntry, KnowledgeError, KnowledgeInputError, KnowledgeTarget,
    LibraryState, NewKnowledgeEntry, PageOptions, PersonRequest, PersonRequestContent,
    PersonRequestError, PersonRequestFilters, PersonRequestSource, SubmittedPersonRequest,
    TaskTarget, VersionedKnowledgeTarget, WriteOperation, approve_person_request,
    change_knowledge_entry, create_knowledge_entry, list_person_requests, read_knowledge_entry,
    read_person_request, submit_person_request,
};
use tokio::sync::{Barrier, mpsc};

use super::helpers::{Gates, Pause, Records, SPACE, next_pid, with_postgres};

#[tokio::test]
async fn approve_person_request_decides_once_when_approved_concurrently() {
    with_postgres(async |fixture| {
        fixture.space().await;
        submit(&fixture.state, &fixture.person, "Guide")
            .await
            .unwrap();
        let held = Pause::default();
        let records = Records::default();
        let first = fixture.state_with(
            Gates {
                before_commit: Some(held.gate()),
                records: Some(records.clone()),
                ..Gates::default()
            },
            false,
        );
        let person = fixture.person.clone();
        let approval = tokio::spawn(async move { approve(&first, &person).await });
        held.reached().await;
        let (report_pid, mut pid) = mpsc::unbounded_channel();
        let second = fixture.state_with(
            Gates {
                report_pid: Some(report_pid),
                ..Gates::default()
            },
            false,
        );
        let person = fixture.person.clone();
        let concurrent = tokio::spawn(async move { approve(&second, &person).await });
        fixture.wait_for_lock(next_pid(&mut pid).await).await;
        held.release();
        assert_eq!(
            approval.await.unwrap().unwrap().details.status.as_str(),
            "approved"
        );
        assert_eq!(
            concurrent.await.unwrap().unwrap_err().code(),
            "person_request.decided"
        );
        let entry = read_knowledge_entry(&fixture.state, &fixture.person, SPACE, target().unwrap())
            .await
            .unwrap();
        assert_eq!(entry.version, 1);
        assert_eq!(entry.content.as_deref(), Some("Proposed"));
        assert_eq!(
            read_person_request(&fixture.state, &fixture.person, SPACE, "RQ-1")
                .await
                .unwrap()
                .details
                .status
                .as_str(),
            "approved"
        );
        assert_eq!(
            records.lock().unwrap()[0].as_ref().unwrap().operation,
            WriteOperation::ApprovePersonRequest
        );
    })
    .await;
}

#[tokio::test]
async fn approve_person_request_keeps_versions_consecutive_when_entry_changes_concurrently() {
    with_postgres(async |fixture| {
        fixture.space().await;
        prepare_change(&fixture.state, &fixture.person)
            .await
            .unwrap();
        let held = Pause::default();
        let records = Records::default();
        let first = fixture.state_with(
            Gates {
                before_commit: Some(held.gate()),
                records: Some(records.clone()),
                ..Gates::default()
            },
            false,
        );
        let person = fixture.person.clone();
        let changing = tokio::spawn(async move { change(&first, &person).await });
        held.reached().await;
        let (report_pid, mut pid) = mpsc::unbounded_channel();
        let second = fixture.state_with(
            Gates {
                report_pid: Some(report_pid),
                ..Gates::default()
            },
            false,
        );
        let person = fixture.person.clone();
        let approval = tokio::spawn(async move { approve(&second, &person).await });
        fixture.wait_for_lock(next_pid(&mut pid).await).await;
        held.release();
        assert_eq!(changing.await.unwrap().unwrap().version, 2);
        assert_eq!(
            approval.await.unwrap().unwrap().details.status.as_str(),
            "approved"
        );
        let entry = read_knowledge_entry(&fixture.state, &fixture.person, SPACE, target().unwrap())
            .await
            .unwrap();
        assert_eq!(entry.version, 3);
        assert_eq!(entry.content.as_deref(), Some("Proposed"));
        assert_eq!(entry.summary.as_deref(), Some("Concurrent"));
        assert_eq!(
            records.lock().unwrap()[0].as_ref().unwrap().operation,
            WriteOperation::ChangeKnowledgeEntry
        );
    })
    .await;
}

#[tokio::test]
async fn submit_person_request_assigns_distinct_keys_when_submitted_concurrently() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Concurrent requests").await;
        let held = Pause::default();
        let records = Records::default();
        let first = fixture.state_with(
            Gates {
                before_commit: Some(held.gate()),
                records: Some(records.clone()),
                ..Gates::default()
            },
            false,
        );
        let person = fixture.person.clone();
        let submission = tokio::spawn(async move { submit(&first, &person, "First").await });
        held.reached().await;
        let (report_pid, mut pid) = mpsc::unbounded_channel();
        let second = fixture.state_with(
            Gates {
                report_pid: Some(report_pid),
                ..Gates::default()
            },
            false,
        );
        let agent = fixture.agent.clone();
        let concurrent =
            tokio::spawn(async move { submit_agent(&second, &agent, task.task_id.expose()).await });
        fixture.wait_for_lock(next_pid(&mut pid).await).await;
        held.release();
        let first = submission.await.unwrap().unwrap();
        assert_eq!(first.person_request.details.key, "RQ-1");
        let second = concurrent.await.unwrap().unwrap();
        assert_eq!(second.person_request.details.key, "RQ-2");
        assert_eq!(second.task_title.as_deref(), Some("Concurrent requests"));
        let submissions = submit_many(&fixture.state, &fixture.person).await.unwrap();
        assert_eq!(
            submissions.into_iter().collect::<BTreeSet<_>>(),
            (3..=22).map(|number| format!("RQ-{number}")).collect()
        );
        let listed = list_person_requests(
            &fixture.state,
            &fixture.person,
            SPACE,
            PageOptions::new(Some(20), None).unwrap(),
            PersonRequestFilters::new(None, None).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(listed.items.len(), 20);
        assert!(listed.next_cursor.is_some());
        assert_eq!(
            records.lock().unwrap()[0].as_ref().unwrap().operation,
            WriteOperation::SubmitPersonRequest
        );
    })
    .await;
}

async fn submit(
    state: &LibraryState,
    caller: &Caller,
    name: &str,
) -> Result<SubmittedPersonRequest, PersonRequestError> {
    state
        .run_write(async |write| {
            submit_person_request(
                write,
                caller,
                PersonRequestSource::Space(SPACE),
                PersonRequestContent::Create(NewKnowledgeEntry::new(
                    name.to_owned(),
                    "Summary".to_owned(),
                    "Proposed".to_owned(),
                )?),
            )
            .await
        })
        .await
}

async fn approve(
    state: &LibraryState,
    caller: &Caller,
) -> Result<PersonRequest, PersonRequestError> {
    state
        .run_write(async |write| approve_person_request(write, caller, SPACE, "RQ-1", 1).await)
        .await
}

fn target() -> Result<KnowledgeTarget, KnowledgeInputError> {
    KnowledgeTarget::new(None, Some("KN-1".to_owned()))
}

async fn prepare_change(state: &LibraryState, caller: &Caller) -> Result<(), PersonRequestError> {
    let input = NewKnowledgeEntry::new(
        "Guide".to_owned(),
        "Initial".to_owned(),
        "Initial".to_owned(),
    )?;
    state
        .run_write(async |write| create_knowledge_entry(write, caller, SPACE, input).await)
        .await?;
    let content = PersonRequestContent::Change(
        target()?,
        KnowledgeChanges::new(None, None, Some("Proposed".to_owned()))?,
    );
    state
        .run_write(async |write| {
            submit_person_request(write, caller, PersonRequestSource::Space(SPACE), content).await
        })
        .await?;
    Ok(())
}

async fn change(state: &LibraryState, caller: &Caller) -> Result<KnowledgeEntry, KnowledgeError> {
    let changes = KnowledgeChanges::new(
        None,
        Some("Concurrent".to_owned()),
        Some("Intervening".to_owned()),
    )?;
    state
        .run_write(async |write| {
            change_knowledge_entry(
                write,
                caller,
                SPACE,
                VersionedKnowledgeTarget::new(target()?, 1)?,
                changes,
            )
            .await
        })
        .await
}

async fn submit_agent(
    state: &LibraryState,
    caller: &Caller,
    task_id: &str,
) -> Result<SubmittedPersonRequest, PersonRequestError> {
    let content = PersonRequestContent::Create(NewKnowledgeEntry::new(
        "Second".to_owned(),
        "Summary".to_owned(),
        "Proposed".to_owned(),
    )?);
    state
        .run_write(async |write| {
            submit_person_request(
                write,
                caller,
                PersonRequestSource::Task(TaskTarget {
                    path_space: None,
                    task_id,
                }),
                content,
            )
            .await
        })
        .await
}

async fn submit_many(
    state: &LibraryState,
    caller: &Caller,
) -> Result<Vec<String>, PersonRequestError> {
    let barrier = Barrier::new(20);
    try_join_all((0..20).map(|number| {
        let barrier = &barrier;
        async move {
            barrier.wait().await;
            Ok(submit(state, caller, &format!("Entry {number}"))
                .await?
                .person_request
                .details
                .key)
        }
    }))
    .await
}
