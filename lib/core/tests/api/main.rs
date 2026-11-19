use std::collections::BTreeSet;
use std::sync::atomic::Ordering;

use futures_util::future::join_all;
use lorebay_lib_core::{
    Caller, NewSpace, NoteContent, SpaceError, SpaceInputError, TaskChanges, TaskStatus, add_note,
    change_task, complete_artifact, create_space, delete_task, get_artifact_download_url,
    highest_space_number, list_tasks, read_task, remove_abandoned_artifacts,
};
use sea_orm::TransactionTrait;
use tokio::sync::{Barrier, mpsc};

use helpers::{
    Gates, Pause, SPACE, append, filters, new_task, next_pid, now_millis, page, register, target,
    wait_until_after, with_postgres,
};

#[expect(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    clippy::unwrap_used,
    reason = "this integration target uses fixture assertions that must fail the scenario"
)]
mod helpers;

#[tokio::test]
async fn create_space_refuses_nonpositive_numbers() {
    with_postgres(async |fixture| {
        for number in [0, -1] {
            let input = NewSpace::new("Research".to_owned(), "Shared work".to_owned()).unwrap();
            let error = fixture
                .state
                .run_write(Some(&fixture.person), async |write| {
                    create_space(write, &fixture.person.owner_id, number, input).await
                })
                .await
                .unwrap_err();
            assert!(matches!(
                error,
                SpaceError::Input(SpaceInputError::InvalidNumber)
            ));
            assert_eq!(error.code(), "request.invalid_body");
        }
        let transaction = fixture.database.begin().await.unwrap();
        assert_eq!(highest_space_number(&transaction).await.unwrap(), None);
        transaction.commit().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn create_task_assigns_distinct_keys_when_created_concurrently() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let barrier = Barrier::new(50);
        let tasks = join_all((0..50).map(|number| {
            let barrier = &barrier;
            async move {
                barrier.wait().await;
                new_task(&fixture.state, &fixture.agent, &format!("Task {number}")).await
            }
        }))
        .await;
        assert_consecutive(tasks.into_iter().map(|task| task.key), "TK");
        let listed = list_tasks(
            &fixture.state,
            &fixture.person,
            SPACE,
            page(),
            filters(None),
        )
        .await
        .unwrap();
        assert_consecutive(listed.items.into_iter().map(|task| task.key), "TK");
        assert!(listed.next_cursor.is_none());
    })
    .await;
}

#[tokio::test]
async fn create_artifact_assigns_distinct_keys_when_registered_concurrently() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let tasks = join_all(
            (0..5).map(|number| async move { fixture.task(&format!("Task {number}")).await }),
        )
        .await;
        let barrier = Barrier::new(50);
        let registrations = join_all((0..50).map(|number| {
            let task = &tasks[number % tasks.len()];
            let barrier = &barrier;
            async move {
                barrier.wait().await;
                (task, register(&fixture.state, &fixture.agent, task).await)
            }
        }))
        .await;
        assert_consecutive(registrations.iter().map(|(_, item)| item.key.clone()), "AR");
        for (task, item) in registrations {
            fixture.storage.upload(&item, b"result");
            complete_artifact(&fixture.state, &fixture.agent, target(task), &item.key)
                .await
                .unwrap();
        }
        let mut keys = Vec::new();
        for task in tasks {
            let read = read_task(&fixture.state, &fixture.person, SPACE, &task.key, None)
                .await
                .unwrap();
            keys.extend(read.artifacts.into_iter().map(|artifact| artifact.key));
        }
        assert_consecutive(keys, "AR");
    })
    .await;
}

#[tokio::test]
async fn add_note_keeps_times_increasing_when_appended_concurrently() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Append concurrently").await;
        let barrier = Barrier::new(50);
        let notes = join_all((0..50).map(|number| {
            let barrier = &barrier;
            let task = &task;
            async move {
                barrier.wait().await;
                append(
                    &fixture.state,
                    &fixture.agent,
                    task,
                    &format!("Note {number}"),
                )
                .await
            }
        }))
        .await;
        let mut times: Vec<i64> = notes.iter().map(|note| note.time).collect();
        times.sort_unstable();
        assert!(times.windows(2).all(|pair| pair[0] < pair[1]));
        let read = read_task(
            &fixture.state,
            &fixture.person,
            SPACE,
            &task.key,
            Some(page()),
        )
        .await
        .unwrap();
        let notes = read.scratchpad.unwrap();
        assert_eq!(
            notes.items.iter().map(|note| note.time).collect::<Vec<_>>(),
            times
        );
        assert_eq!(read.task.last_activity_at, *times.last().unwrap());
    })
    .await;
}

#[tokio::test]
async fn add_note_fails_when_task_ends_concurrently() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Close with append waiting").await;
        let held = Pause::default();
        let closer = fixture.state_with(
            Gates {
                before_commit: Some(held.gate()),
                ..Gates::default()
            },
            false,
        );
        let issuer = fixture.person.clone();
        let closing = task.clone();
        let close = tokio::spawn(async move {
            closer
                .run_write(Some(&issuer), async |write| {
                    change_task(write, &issuer, target(&closing), ended()).await
                })
                .await
        });
        held.reached().await;

        let (report_pid, mut pid) = mpsc::unbounded_channel();
        let appender = fixture.state_with(
            Gates {
                report_pid: Some(report_pid),
                ..Gates::default()
            },
            false,
        );
        let writer = fixture.agent.clone();
        let appending = task.clone();
        let append = tokio::spawn(async move {
            appender
                .run_write(Some(&writer), async |write| {
                    let content = NoteContent::try_from("After closure".to_owned()).unwrap();
                    add_note(write, &writer, target(&appending), content).await
                })
                .await
        });
        fixture.wait_for_lock(next_pid(&mut pid).await).await;
        held.release();
        assert_eq!(close.await.unwrap().unwrap().task.status, TaskStatus::Ended);
        let error = append.await.unwrap().unwrap_err();
        assert_eq!(error.code(), "task.ended");
        let read = read_task(
            &fixture.state,
            &fixture.person,
            SPACE,
            &task.key,
            Some(page()),
        )
        .await
        .unwrap();
        assert_eq!(read.task.status, TaskStatus::Ended);
        assert!(read.scratchpad.unwrap().items.is_empty());
    })
    .await;
}

#[tokio::test]
async fn complete_artifact_keeps_last_activity_when_note_is_newer() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Completion and newer note").await;
        let artifact = register(&fixture.state, &fixture.agent, &task).await;
        fixture.storage.upload(&artifact, b"file");
        let complete_pause = Pause::default();
        let (report_pid, mut pid) = mpsc::unbounded_channel();
        let completion_state = fixture.state_with(
            Gates {
                before_begin: Some(complete_pause.gate()),
                report_pid: Some(report_pid),
                ..Gates::default()
            },
            false,
        );
        let agent = fixture.agent.clone();
        let completing = task.clone();
        let complete = tokio::spawn(async move {
            complete_artifact(
                &completion_state,
                &agent,
                target(&completing),
                &artifact.key,
            )
            .await
        });
        complete_pause.reached().await;
        let older = now_millis();
        wait_until_after(older).await;

        let note_pause = Pause::default();
        let note_state = fixture.state_with(
            Gates {
                before_commit: Some(note_pause.gate()),
                ..Gates::default()
            },
            false,
        );
        let agent = fixture.agent.clone();
        let appending = task.clone();
        let note =
            tokio::spawn(
                async move { append(&note_state, &agent, &appending, "Newer note").await },
            );
        note_pause.reached().await;
        complete_pause.release();
        fixture.wait_for_lock(next_pid(&mut pid).await).await;
        note_pause.release();
        let note = note.await.unwrap();
        assert!(note.time > older);
        assert_eq!(complete.await.unwrap().unwrap().artifact.size, 4);
        let read = fixture.read(&task).await;
        assert_eq!(read.task.last_activity_at, note.time);
        assert_eq!(read.artifacts.len(), 1);
    })
    .await;
}

#[tokio::test]
async fn remove_abandoned_artifacts_keeps_artifact_when_completed_concurrently() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Cleanup and completion").await;
        let item = register(&fixture.state, &fixture.agent, &task).await;
        let location = fixture.storage.upload(&item, b"kept");
        let complete_pause = Pause::default();
        let completer = fixture.state_with(
            Gates {
                before_commit: Some(complete_pause.gate()),
                ..Gates::default()
            },
            false,
        );
        let agent = fixture.agent.clone();
        let completing = task.clone();
        let complete = tokio::spawn(async move {
            complete_artifact(&completer, &agent, target(&completing), &item.key).await
        });
        complete_pause.reached().await;
        wait_until_after(now_millis() + 2).await;
        let (report_pid, mut pid) = mpsc::unbounded_channel();
        let cleaner = fixture.state_with(
            Gates {
                report_pid: Some(report_pid),
                ..Gates::default()
            },
            true,
        );
        let cleanup = tokio::spawn(async move { remove_abandoned_artifacts(&cleaner).await });
        fixture.wait_for_lock(next_pid(&mut pid).await).await;
        complete_pause.release();
        complete.await.unwrap().unwrap();
        assert_eq!(cleanup.await.unwrap().unwrap(), 0);
        let read = read_task(&fixture.state, &fixture.person, SPACE, &task.key, None)
            .await
            .unwrap();
        assert_eq!(read.artifacts[0].key, "AR-1");
        assert!(fixture.storage.contains(&location, b"kept"));
    })
    .await;
}

#[tokio::test]
async fn list_tasks_matches_keyword_when_ascii_case_differs() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let matched = fixture.task("Build A%_B\\ \u{00C4}").await;
        fixture.task("Build AxxB/ \u{00E4}").await;
        let result = fixture.task("Other title").await;
        fixture
            .state
            .run_write(Some(&fixture.agent), async |write| {
                change_task(
                    write,
                    &fixture.agent,
                    target(&result),
                    TaskChanges {
                        title: None,
                        status: None,
                        result: Some("BUILD done".to_owned().try_into().unwrap()),
                    },
                )
                .await
            })
            .await
            .unwrap();
        for (keyword, expected) in [
            ("build", vec!["TK-1", "TK-2", "TK-3"]),
            ("a%_b\\", vec![matched.key.as_str()]),
            ("%", vec![matched.key.as_str()]),
            ("_", vec![matched.key.as_str()]),
            ("\\", vec![matched.key.as_str()]),
            ("\u{00C4}", vec![matched.key.as_str()]),
            ("\u{00E4}", vec!["TK-2"]),
        ] {
            let listed = list_tasks(
                &fixture.state,
                &fixture.person,
                SPACE,
                page(),
                filters(Some(keyword)),
            )
            .await
            .unwrap();
            let keys: BTreeSet<String> = listed.items.into_iter().map(|item| item.key).collect();
            assert_eq!(keys, expected.into_iter().map(str::to_owned).collect());
        }
    })
    .await;
}

#[tokio::test]
async fn delete_task_keeps_rows_and_files_when_product_rolls_back() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Rolled back task deletion").await;
        let note = append(&fixture.state, &fixture.agent, &task, "Keep this note").await;
        let artifact = register(&fixture.state, &fixture.agent, &task).await;
        let location = fixture.storage.upload(&artifact, b"keep");
        complete_artifact(&fixture.state, &fixture.agent, target(&task), &artifact.key)
            .await
            .unwrap();

        let outcome: anyhow::Result<()> = fixture
            .state
            .run_write(Some(&fixture.person), async |write| {
                delete_task(write, &fixture.person, target(&task)).await?;
                assert!(fixture.storage.contains(&location, b"keep"));
                anyhow::bail!("product deliberately abandoned the write");
            })
            .await;
        assert_eq!(
            outcome.unwrap_err().to_string(),
            "product deliberately abandoned the write"
        );

        let read = read_task(
            &fixture.state,
            &fixture.person,
            SPACE,
            &task.key,
            Some(page()),
        )
        .await
        .unwrap();
        assert_eq!(read.task.key, task.key);
        let scratchpad = read.scratchpad.unwrap();
        assert_eq!(scratchpad.items[0].time, note.time);
        assert_eq!(scratchpad.items[0].content, "Keep this note");
        assert_eq!(read.artifacts[0].key, artifact.key);
        let download =
            get_artifact_download_url(&fixture.state, &fixture.person, SPACE, &artifact.key)
                .await
                .unwrap();
        assert_eq!(download.url.expose(), &location);
        assert!(fixture.storage.contains(&location, b"keep"));
    })
    .await;
}

#[tokio::test]
async fn get_task_fails_as_not_found_when_space_has_another_owner() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Keep scope private").await;
        let outside = Caller {
            owner_id: "other".to_owned(),
            ..fixture.agent.clone()
        };
        let roles_before = fixture.roles.0.load(Ordering::Relaxed);
        let read = read_task(&fixture.state, &outside, SPACE, &task.key, None)
            .await
            .unwrap_err();
        assert_eq!(read.code(), "space.not_found");
        let write = fixture
            .state
            .run_write(Some(&outside), async |write| {
                change_task(write, &outside, target(&task), ended()).await
            })
            .await
            .unwrap_err();
        assert_eq!(write.code(), "task.not_found");
        assert_eq!(fixture.roles.0.load(Ordering::Relaxed), roles_before);
        let read = read_task(&fixture.state, &fixture.person, SPACE, &task.key, None)
            .await
            .unwrap();
        assert_eq!(read.task.status, TaskStatus::InProgress);
    })
    .await;
}

fn assert_consecutive(keys: impl IntoIterator<Item = String>, prefix: &str) {
    let keys: Vec<_> = keys.into_iter().collect();
    assert_eq!(keys.len(), 50);
    let expected: BTreeSet<_> = (1..=50)
        .map(|number| format!("{prefix}-{number}"))
        .collect();
    assert_eq!(keys.into_iter().collect::<BTreeSet<_>>(), expected);
}

fn ended() -> TaskChanges {
    TaskChanges {
        title: None,
        status: Some(TaskStatus::Ended),
        result: None,
    }
}
