use std::collections::BTreeSet;

use futures_util::future::try_join_all;
use lorebay_lib_core::{
    Caller, KnowledgeChanges, KnowledgeEntry, KnowledgeError, KnowledgeInputError, KnowledgeTarget,
    LibraryState, NewKnowledgeEntry, PageOptions, Proposal, ProposalContent, ProposalError,
    ProposalFilters, ProposalSource, SubmittedProposal, TaskTarget, WriteOperation,
    approve_proposal, change_knowledge_entry, create_knowledge_entry, list_proposals,
    read_knowledge_entry, read_proposal, submit_proposal,
};
use tokio::sync::{Barrier, mpsc};

use super::helpers::{Gates, Pause, Records, SPACE, next_pid, with_postgres};

#[tokio::test]
async fn approve_proposal_decides_once_when_approved_concurrently() {
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
            "proposal.decided"
        );
        let entry = read_knowledge_entry(&fixture.state, &fixture.person, SPACE, target().unwrap())
            .await
            .unwrap();
        assert_eq!(entry.version, 1);
        assert_eq!(entry.content.as_deref(), Some("Proposed"));
        assert_eq!(
            read_proposal(&fixture.state, &fixture.person, SPACE, "PP-1")
                .await
                .unwrap()
                .details
                .status
                .as_str(),
            "approved"
        );
        assert_eq!(
            records.lock().unwrap()[0].as_ref().unwrap().operation,
            WriteOperation::ApproveProposal
        );
    })
    .await;
}

#[tokio::test]
async fn approve_proposal_keeps_versions_consecutive_when_entry_changes_concurrently() {
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
async fn submit_proposal_assigns_distinct_keys_when_submitted_concurrently() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Concurrent proposals").await;
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
        assert_eq!(
            submission.await.unwrap().unwrap().proposal.details.key,
            "PP-1"
        );
        let second = concurrent.await.unwrap().unwrap();
        assert_eq!(second.proposal.details.key, "PP-2");
        assert_eq!(second.task_title.as_deref(), Some("Concurrent proposals"));
        let submissions = submit_many(&fixture.state, &fixture.person).await.unwrap();
        assert_eq!(
            submissions.into_iter().collect::<BTreeSet<_>>(),
            (3..=22).map(|number| format!("PP-{number}")).collect()
        );
        let listed = list_proposals(
            &fixture.state,
            &fixture.person,
            SPACE,
            PageOptions::new(Some(20), None).unwrap(),
            ProposalFilters::new(None, None).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(listed.items.len(), 20);
        assert!(listed.next_cursor.is_some());
        assert_eq!(
            records.lock().unwrap()[0].as_ref().unwrap().operation,
            WriteOperation::SubmitProposal
        );
    })
    .await;
}

async fn submit(
    state: &LibraryState,
    caller: &Caller,
    name: &str,
) -> Result<SubmittedProposal, ProposalError> {
    state
        .run_write(async |write| {
            submit_proposal(
                write,
                caller,
                ProposalSource::Space(SPACE),
                ProposalContent::Create(NewKnowledgeEntry::new(
                    name.to_owned(),
                    "Summary".to_owned(),
                    "Proposed".to_owned(),
                )?),
            )
            .await
        })
        .await
}

async fn approve(state: &LibraryState, caller: &Caller) -> Result<Proposal, ProposalError> {
    state
        .run_write(async |write| approve_proposal(write, caller, SPACE, "PP-1").await)
        .await
}

fn target() -> Result<KnowledgeTarget, KnowledgeInputError> {
    KnowledgeTarget::new(None, Some("KN-1".to_owned()))
}

async fn prepare_change(state: &LibraryState, caller: &Caller) -> Result<(), ProposalError> {
    let input = NewKnowledgeEntry::new(
        "Guide".to_owned(),
        "Initial".to_owned(),
        "Initial".to_owned(),
    )?;
    state
        .run_write(async |write| create_knowledge_entry(write, caller, SPACE, input).await)
        .await?;
    let content = ProposalContent::Change(
        target()?,
        KnowledgeChanges::new(None, None, Some("Proposed".to_owned()))?,
    );
    state
        .run_write(async |write| {
            submit_proposal(write, caller, ProposalSource::Space(SPACE), content).await
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
            change_knowledge_entry(write, caller, SPACE, target()?, changes).await
        })
        .await
}

async fn submit_agent(
    state: &LibraryState,
    caller: &Caller,
    task_id: &str,
) -> Result<SubmittedProposal, ProposalError> {
    let content = ProposalContent::Create(NewKnowledgeEntry::new(
        "Second".to_owned(),
        "Summary".to_owned(),
        "Proposed".to_owned(),
    )?);
    state
        .run_write(async |write| {
            submit_proposal(
                write,
                caller,
                ProposalSource::Task(TaskTarget {
                    path_space: None,
                    task_id,
                }),
                content,
            )
            .await
        })
        .await
}

async fn submit_many(state: &LibraryState, caller: &Caller) -> Result<Vec<String>, ProposalError> {
    let barrier = Barrier::new(20);
    try_join_all((0..20).map(|number| {
        let barrier = &barrier;
        async move {
            barrier.wait().await;
            Ok(submit(state, caller, &format!("Entry {number}"))
                .await?
                .proposal
                .details
                .key)
        }
    }))
    .await
}
