use lorebay_lib_core::{
    Caller, KnowledgeChanges, KnowledgeEntry, KnowledgeError, KnowledgeFilters,
    KnowledgeInputError, KnowledgeTarget, LibraryState, NewKnowledgeEntry,
    VersionedKnowledgeTarget, WriteOperation, change_knowledge_entry, create_knowledge_entry,
    list_knowledge, read_knowledge_entry,
};
use tokio::sync::mpsc;

use super::helpers::{Gates, Pause, Records, SPACE, next_pid, page, with_postgres};

#[tokio::test]
async fn create_knowledge_entry_keeps_one_entry_when_one_name_is_created_concurrently() {
    with_postgres(async |fixture| {
        fixture.space().await;
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
        let creation =
            tokio::spawn(async move { create(&first, &person, "Guide", "First", "Content").await });
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
        let duplicate =
            tokio::spawn(
                async move { create(&second, &person, "guide", "Second", "Content").await },
            );
        fixture.wait_for_lock(next_pid(&mut pid).await).await;
        held.release();
        assert_eq!(creation.await.unwrap().unwrap().key, "KN-1");
        assert_eq!(
            duplicate.await.unwrap().unwrap_err().code(),
            "knowledge.name_taken"
        );
        let listed = list_knowledge(
            &fixture.state,
            &fixture.person,
            SPACE,
            page(),
            KnowledgeFilters::new(None, None).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(listed.items.len(), 1);
        assert_eq!(listed.items[0].summary.as_deref(), Some("First"));
        assert_eq!(
            records.lock().unwrap()[0].as_ref().unwrap().operation,
            WriteOperation::CreateKnowledgeEntry
        );
    })
    .await;
}

#[tokio::test]
async fn change_knowledge_entry_numbers_versions_consecutively_when_changed_concurrently() {
    with_postgres(async |fixture| {
        fixture.space().await;
        create(&fixture.state, &fixture.person, "Guide", "Initial", "Keep")
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
        let changing =
            tokio::spawn(async move { change(&first, &person, None, Some("First"), 1).await });
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
        let concurrent =
            tokio::spawn(async move { change(&second, &person, Some("Latest"), None, 2).await });
        fixture.wait_for_lock(next_pid(&mut pid).await).await;
        held.release();
        assert_eq!(changing.await.unwrap().unwrap().version, 2);
        let changed = concurrent.await.unwrap().unwrap();
        assert_eq!(changed.version, 3);
        assert_eq!(changed.summary.as_deref(), Some("First"));
        assert_eq!(changed.content.as_deref(), Some("Latest"));
        let read = read_knowledge_entry(&fixture.state, &fixture.person, SPACE, target().unwrap())
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(read).unwrap(),
            serde_json::to_value(changed).unwrap()
        );
        assert_eq!(
            records.lock().unwrap()[0].as_ref().unwrap().operation,
            WriteOperation::ChangeKnowledgeEntry
        );
    })
    .await;
}

#[tokio::test]
async fn list_knowledge_matches_keyword_when_ascii_case_differs() {
    with_postgres(async |fixture| {
        fixture.space().await;
        create(
            &fixture.state,
            &fixture.person,
            "BUILD A%_B\\ \u{00C4}",
            "First",
            "Body",
        )
        .await
        .unwrap();
        create(
            &fixture.state,
            &fixture.person,
            "Other",
            "BUILD AxxB/ \u{00E4}",
            "Body",
        )
        .await
        .unwrap();
        create(
            &fixture.state,
            &fixture.person,
            "Third",
            "Summary",
            "BUILD content",
        )
        .await
        .unwrap();
        for (keyword, expected) in [
            ("build", vec!["KN-1", "KN-2", "KN-3"]),
            ("a%_b\\", vec!["KN-1"]),
            ("%", vec!["KN-1"]),
            ("_", vec!["KN-1"]),
            ("\\", vec!["KN-1"]),
            ("\u{00C4}", vec!["KN-1"]),
            ("\u{00E4}", vec!["KN-2"]),
        ] {
            let listed = list_knowledge(
                &fixture.state,
                &fixture.person,
                SPACE,
                page(),
                KnowledgeFilters::new(None, Some(keyword.to_owned())).unwrap(),
            )
            .await
            .unwrap();
            let mut keys: Vec<_> = listed
                .items
                .into_iter()
                .map(|item| item.key.unwrap())
                .collect();
            keys.sort();
            assert_eq!(keys, expected);
        }
    })
    .await;
}

async fn create(
    state: &LibraryState,
    caller: &Caller,
    name: &str,
    summary: &str,
    content: &str,
) -> Result<KnowledgeEntry, KnowledgeError> {
    state
        .run_write(async |write| {
            create_knowledge_entry(
                write,
                caller,
                SPACE,
                NewKnowledgeEntry::new(name.to_owned(), summary.to_owned(), content.to_owned())?,
            )
            .await
        })
        .await
}

async fn change(
    state: &LibraryState,
    caller: &Caller,
    content: Option<&str>,
    summary: Option<&str>,
    version: i64,
) -> Result<KnowledgeEntry, KnowledgeError> {
    let changes =
        KnowledgeChanges::new(None, summary.map(str::to_owned), content.map(str::to_owned))?;
    state
        .run_write(async |write| {
            change_knowledge_entry(
                write,
                caller,
                SPACE,
                VersionedKnowledgeTarget::new(target()?, version)?,
                changes,
            )
            .await
        })
        .await
}

fn target() -> Result<KnowledgeTarget, KnowledgeInputError> {
    KnowledgeTarget::new(None, Some("KN-1".to_owned()))
}
