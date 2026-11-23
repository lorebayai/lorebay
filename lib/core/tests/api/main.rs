use std::collections::BTreeSet;
use std::sync::atomic::Ordering;

use futures_util::future::join_all;
use lorebay_lib_core::{
    Caller, CheckpointContent, NewSpace, SpaceError, SpaceInputError, SpaceRole, TaskChanges,
    TaskStatus, WriteOperation, add_checkpoint, change_task,
    complete_agent_file as record_completion, create_space, delete_task,
    get_agent_file_download_url, highest_space_number, list_tasks, prepare_agent_file_completion,
    read_task, register_agent_file, remove_orphaned_files, resolve_space,
};
use sea_orm::TransactionTrait;
use tokio::sync::{Barrier, mpsc};

use helpers::{
    Gates, Pause, Records, SPACE, agent_file_input, append,
    cleanup as remove_abandoned_agent_files, complete as complete_agent_file,
    delete as remove_agent_file, filters, new_task, next_pid, now_millis, page, register,
    spawn_completion, target, wait_until_after, with_postgres,
};

#[expect(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    clippy::unwrap_used,
    reason = "this integration target uses fixture assertions that must fail the scenario"
)]
mod helpers;
mod knowledge;
mod person_requests;

#[tokio::test]
async fn complete_agent_file_refuses_registration_after_oversize_report() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Final oversize refusal").await;
        let item = register(&fixture.state, &fixture.agent, &task).await;
        let location = fixture.storage.upload(&item, &[0; 101]);
        fixture.storage.fail_next_delete();
        let error = complete_agent_file(&fixture.state, &fixture.agent, target(&task), &item.key)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "agent_file.too_large");
        assert!(fixture.storage.contains(&location, &[0; 101]));
        let error = complete_agent_file(&fixture.state, &fixture.agent, target(&task), &item.key)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "agent_file.refused");
        assert!(fixture.read(&task).await.files.is_empty());
        let cleaner = fixture.state_with(Gates::default(), true);
        wait_until_after(now_millis() + 2).await;
        assert_eq!(remove_abandoned_agent_files(&cleaner).await.unwrap(), 0);
        fixture.storage.age();
        assert_eq!(remove_orphaned_files(&fixture.state).await.unwrap(), 1);
        assert!(!fixture.storage.contains(&location, &[0; 101]));
        fixture.storage.upload(&item, b"retry");
        let error = complete_agent_file(&fixture.state, &fixture.agent, target(&task), &item.key)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "agent_file.refused");
        assert!(fixture.storage.contains(&location, b"retry"));
    })
    .await;
}

#[tokio::test]
async fn create_space_refuses_nonpositive_numbers() {
    with_postgres(async |fixture| {
        for number in [0, -1] {
            let input = NewSpace::new("Research".to_owned(), "Shared work".to_owned()).unwrap();
            let error = fixture
                .state
                .run_write(async |write| {
                    create_space(write, &fixture.person, number, SpaceRole::Owner, input).await
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
async fn create_agent_file_assigns_distinct_keys_when_registered_concurrently() {
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
        assert_consecutive(registrations.iter().map(|(_, item)| item.key.clone()), "FL");
        for (task, item) in registrations {
            fixture.storage.upload(&item, b"result");
            complete_agent_file(&fixture.state, &fixture.agent, target(task), &item.key)
                .await
                .unwrap();
        }
        let mut keys = Vec::new();
        for task in tasks {
            let read = read_task(&fixture.state, &fixture.person, SPACE, &task.key, None)
                .await
                .unwrap();
            keys.extend(read.files.into_iter().map(|agent_file| agent_file.key));
        }
        assert_consecutive(keys, "FL");
    })
    .await;
}

#[tokio::test]
async fn add_checkpoint_keeps_times_increasing_when_appended_concurrently() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Append concurrently").await;
        let barrier = Barrier::new(50);
        let checkpoints = join_all((0..50).map(|number| {
            let barrier = &barrier;
            let task = &task;
            async move {
                barrier.wait().await;
                append(
                    &fixture.state,
                    &fixture.agent,
                    task,
                    &format!("Checkpoint {number}"),
                )
                .await
            }
        }))
        .await;
        let mut times: Vec<i64> = checkpoints
            .iter()
            .map(|checkpoint| checkpoint.time)
            .collect();
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
        let checkpoints = read.scratchpad.unwrap();
        assert_eq!(
            checkpoints
                .items
                .iter()
                .map(|checkpoint| checkpoint.time)
                .collect::<Vec<_>>(),
            times
        );
        assert_eq!(read.task.last_activity_at, *times.last().unwrap());
    })
    .await;
}

#[tokio::test]
async fn add_checkpoint_fails_when_task_ends_concurrently() {
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
                .run_write(async |write| {
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
                .run_write(async |write| {
                    let content = CheckpointContent::try_from("After closure".to_owned()).unwrap();
                    add_checkpoint(write, &writer, target(&appending), content).await
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
async fn complete_agent_file_keeps_last_activity_when_checkpoint_is_newer() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Completion and newer checkpoint").await;
        let agent_file = register(&fixture.state, &fixture.agent, &task).await;
        fixture.storage.upload(&agent_file, b"file");
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
            complete_agent_file(
                &completion_state,
                &agent,
                target(&completing),
                &agent_file.key,
            )
            .await
        });
        complete_pause.reached().await;
        let older = now_millis();
        wait_until_after(older).await;

        let checkpoint_pause = Pause::default();
        let checkpoint_state = fixture.state_with(
            Gates {
                before_commit: Some(checkpoint_pause.gate()),
                ..Gates::default()
            },
            false,
        );
        let agent = fixture.agent.clone();
        let appending = task.clone();
        let checkpoint = tokio::spawn(async move {
            append(&checkpoint_state, &agent, &appending, "Newer checkpoint").await
        });
        checkpoint_pause.reached().await;
        complete_pause.release();
        fixture.wait_for_lock(next_pid(&mut pid).await).await;
        checkpoint_pause.release();
        let checkpoint = checkpoint.await.unwrap();
        assert!(checkpoint.time > older);
        assert_eq!(complete.await.unwrap().unwrap().agent_file.size, 4);
        let read = fixture.read(&task).await;
        assert_eq!(read.task.last_activity_at, checkpoint.time);
        assert_eq!(read.files.len(), 1);
    })
    .await;
}

#[tokio::test]
async fn remove_abandoned_agent_files_keeps_agent_file_when_completed_concurrently() {
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
            complete_agent_file(&completer, &agent, target(&completing), &item.key).await
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
        let cleanup = tokio::spawn(async move { remove_abandoned_agent_files(&cleaner).await });
        fixture.wait_for_lock(next_pid(&mut pid).await).await;
        complete_pause.release();
        complete.await.unwrap().unwrap();
        assert_eq!(cleanup.await.unwrap().unwrap(), 0);
        let read = read_task(&fixture.state, &fixture.person, SPACE, &task.key, None)
            .await
            .unwrap();
        assert_eq!(read.files[0].key, "FL-1");
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
            .run_write(async |write| {
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
        let checkpoint = append(
            &fixture.state,
            &fixture.agent,
            &task,
            "Keep this checkpoint",
        )
        .await;
        let agent_file = register(&fixture.state, &fixture.agent, &task).await;
        let location = fixture.storage.upload(&agent_file, b"keep");
        complete_agent_file(
            &fixture.state,
            &fixture.agent,
            target(&task),
            &agent_file.key,
        )
        .await
        .unwrap();

        let outcome: anyhow::Result<()> = fixture
            .state
            .run_write(async |write| {
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
        assert_eq!(scratchpad.items[0].time, checkpoint.time);
        assert_eq!(scratchpad.items[0].content, "Keep this checkpoint");
        assert_eq!(read.files[0].key, agent_file.key);
        let download =
            get_agent_file_download_url(&fixture.state, &fixture.person, SPACE, &agent_file.key)
                .await
                .unwrap();
        assert_eq!(download.url.expose(), &location);
        assert!(fixture.storage.contains(&location, b"keep"));
    })
    .await;
}

#[tokio::test]
async fn get_task_fails_as_not_found_when_space_has_another_tenant() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Keep tenant private").await;
        let outside = Caller {
            tenant_id: "other".to_owned(),
            ..fixture.agent.clone()
        };
        let roles_before = fixture.roles.0.load(Ordering::Relaxed);
        let read = read_task(&fixture.state, &outside, SPACE, &task.key, None)
            .await
            .unwrap_err();
        assert_eq!(read.code(), "space.not_found");
        let write = fixture
            .state
            .run_write(async |write| change_task(write, &outside, target(&task), ended()).await)
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

#[tokio::test]
async fn complete_agent_file_refuses_second_report_when_oversize_reports_race() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Concurrent oversize reports").await;
        let item = register(&fixture.state, &fixture.agent, &task).await;
        let location = fixture.storage.upload(&item, &[0; 101]);
        let held = Pause::default();
        let first = fixture.state_with(
            Gates {
                before_commit: Some(held.gate()),
                ..Gates::default()
            },
            false,
        );
        let report = spawn_completion(first, &fixture.agent, &task, &item.key);
        held.reached().await;
        let (report_pid, mut pid) = mpsc::unbounded_channel();
        let second = fixture.state_with(
            Gates {
                report_pid: Some(report_pid),
                ..Gates::default()
            },
            false,
        );
        let duplicate = spawn_completion(second, &fixture.agent, &task, &item.key);
        fixture.wait_for_lock(next_pid(&mut pid).await).await;
        held.release();
        assert_eq!(
            report.await.unwrap().unwrap_err().code(),
            "agent_file.too_large"
        );
        assert_eq!(
            duplicate.await.unwrap().unwrap_err().code(),
            "agent_file.refused"
        );
        assert_eq!(fixture.storage.deletes(), 1);
        assert!(!fixture.storage.contains(&location, &[0; 101]));
        assert!(fixture.read(&task).await.files.is_empty());
    })
    .await;
}

#[tokio::test]
async fn complete_agent_file_preserves_replacement_when_refusal_delete_is_delayed() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Delayed refusal deletion").await;
        let item = register(&fixture.state, &fixture.agent, &task).await;
        let location = fixture.storage.upload(&item, &[0; 101]);
        let pending = Pause::default();
        let stale = fixture.state_with(
            Gates {
                before_begin: Some(pending.gate()),
                ..Gates::default()
            },
            false,
        );
        let stale = spawn_completion(stale, &fixture.agent, &task, &item.key);
        pending.reached().await;
        let deleting = Pause::default();
        fixture.storage.pause_delete(deleting.gate());
        let first = fixture.state.clone();
        let report = spawn_completion(first, &fixture.agent, &task, &item.key);
        deleting.reached().await;
        pending.release();
        assert_eq!(
            stale.await.unwrap().unwrap_err().code(),
            "agent_file.refused"
        );
        let replacement = register(&fixture.state, &fixture.agent, &task).await;
        let replacement_location = fixture.storage.upload(&replacement, b"valid");
        assert_ne!(replacement_location, location);
        let completed = complete_agent_file(
            &fixture.state,
            &fixture.agent,
            target(&task),
            &replacement.key,
        )
        .await
        .unwrap();
        assert_eq!(completed.agent_file.size, 5);
        deleting.release();
        assert_eq!(
            report.await.unwrap().unwrap_err().code(),
            "agent_file.too_large"
        );
        let read = fixture.read(&task).await;
        assert_eq!(read.files.len(), 1);
        assert_eq!(read.files[0].key, replacement.key);
        assert_eq!(read.files[0].size, 5);
        let download =
            get_agent_file_download_url(&fixture.state, &fixture.person, SPACE, &replacement.key)
                .await
                .unwrap();
        assert_eq!(download.url.expose(), &replacement_location);
        assert!(fixture.storage.contains(&replacement_location, b"valid"));
        assert!(!fixture.storage.contains(&location, &[0; 101]));
        assert_eq!(fixture.storage.deletes(), 1);
    })
    .await;
}

#[tokio::test]
async fn register_agent_file_fails_when_task_is_deleted_concurrently() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Delete before registration").await;
        let held = Pause::default();
        let deleter = fixture.state_with(
            Gates {
                before_commit: Some(held.gate()),
                ..Gates::default()
            },
            false,
        );
        let person = fixture.person.clone();
        let deleting = task.clone();
        let deletion = tokio::spawn(async move {
            deleter
                .run_write(async |write| delete_task(write, &person, target(&deleting)).await)
                .await
        });
        held.reached().await;
        let (report_pid, mut pid) = mpsc::unbounded_channel();
        let registrar = fixture.state_with(
            Gates {
                report_pid: Some(report_pid),
                ..Gates::default()
            },
            false,
        );
        let agent = fixture.agent.clone();
        let registering = task.clone();
        let registration = tokio::spawn(async move {
            registrar
                .run_write(async |write| {
                    register_agent_file(write, &agent, target(&registering), agent_file_input())
                        .await
                })
                .await
        });
        fixture.wait_for_lock(next_pid(&mut pid).await).await;
        held.release();
        deletion.await.unwrap().unwrap();
        assert_eq!(
            registration.await.unwrap().unwrap_err().code(),
            "task.not_found"
        );
        assert_eq!(
            read_task(&fixture.state, &fixture.person, SPACE, &task.key, None)
                .await
                .unwrap_err()
                .code(),
            "task.not_found"
        );
        assert_eq!(fixture.storage.deletes(), 0);
    })
    .await;
}

#[tokio::test]
async fn complete_agent_file_fails_when_agent_file_is_deleted_concurrently() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Delete before repeat completion").await;
        let item = register(&fixture.state, &fixture.agent, &task).await;
        let location = fixture.storage.upload(&item, b"file");
        let pending = Pause::default();
        let first = fixture.state_with(
            Gates {
                before_commit: Some(pending.gate()),
                ..Gates::default()
            },
            false,
        );
        let completion = spawn_completion(first, &fixture.agent, &task, &item.key);
        pending.reached().await;
        let error = remove_agent_file(&fixture.state, &fixture.person, target(&task), &item.key)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "agent_file.not_found");
        assert!(fixture.storage.contains(&location, b"file"));
        pending.release();
        assert_eq!(completion.await.unwrap().unwrap().agent_file.key, item.key);
        let held = Pause::default();
        let deleter = fixture.state_with(
            Gates {
                before_commit: Some(held.gate()),
                ..Gates::default()
            },
            false,
        );
        let person = fixture.person.clone();
        let deleting = task.clone();
        let key = item.key.clone();
        let deletion = tokio::spawn(async move {
            remove_agent_file(&deleter, &person, target(&deleting), &key).await
        });
        held.reached().await;
        let (report_pid, mut pid) = mpsc::unbounded_channel();
        let completer = fixture.state_with(
            Gates {
                report_pid: Some(report_pid),
                ..Gates::default()
            },
            false,
        );
        let completion = spawn_completion(completer, &fixture.agent, &task, &item.key);
        fixture.wait_for_lock(next_pid(&mut pid).await).await;
        held.release();
        deletion.await.unwrap().unwrap();
        assert_eq!(
            completion.await.unwrap().unwrap_err().code(),
            "agent_file.not_found"
        );
        assert!(fixture.read(&task).await.files.is_empty());
        assert!(!fixture.storage.contains(&location, b"file"));
    })
    .await;
}

#[tokio::test]
async fn complete_agent_file_records_caller_once_when_reports_race() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("One completion record").await;
        let item = register(&fixture.state, &fixture.agent, &task).await;
        let location = fixture.storage.upload(&item, b"file");
        let records = Records::default();
        let held = Pause::default();
        let first = fixture.state_with(
            Gates {
                before_commit: Some(held.gate()),
                records: Some(records.clone()),
                ..Gates::default()
            },
            false,
        );
        let report = spawn_completion(first, &fixture.agent, &task, &item.key);
        held.reached().await;
        let (report_pid, mut pid) = mpsc::unbounded_channel();
        let second = fixture.state_with(
            Gates {
                report_pid: Some(report_pid),
                records: Some(records.clone()),
                ..Gates::default()
            },
            false,
        );
        let duplicate = spawn_completion(second, &fixture.agent, &task, &item.key);
        fixture.wait_for_lock(next_pid(&mut pid).await).await;
        held.release();
        let first = report.await.unwrap().unwrap();
        let second = duplicate.await.unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(first).unwrap(),
            serde_json::to_value(second).unwrap()
        );
        let records = records.lock().unwrap().clone();
        assert_eq!(records.len(), 2);
        assert_eq!(records.iter().filter(|record| record.is_none()).count(), 1);
        let record = records.iter().flatten().next().unwrap();
        assert_eq!(record.operation, WriteOperation::CompleteAgentFile);
        let caller = record.caller.as_ref().unwrap();
        assert_eq!(caller.person_id, fixture.agent.person_id);
        assert_eq!(caller.agent_name, fixture.agent.agent_name);
        assert_eq!(caller.tenant_id, fixture.agent.tenant_id);
        let transaction = fixture.database.begin().await.unwrap();
        let space = resolve_space(&transaction, &fixture.person, SPACE)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        assert_eq!(record.space_id.as_deref(), Some(space.id.as_str()));
        assert_eq!(fixture.read(&task).await.files.len(), 1);
        assert!(fixture.storage.contains(&location, b"file"));
    })
    .await;
}

#[tokio::test]
async fn run_write_preserves_registration_and_refusal_atomicity_when_product_abandons() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let task = fixture.task("Compose file writes").await;
        let records = Records::default();
        let state = fixture.state_with(
            Gates {
                records: Some(records.clone()),
                ..Gates::default()
            },
            false,
        );
        let registration: anyhow::Result<()> = state
            .run_write(async |write| {
                register_agent_file(write, &fixture.agent, target(&task), agent_file_input())
                    .await?;
                anyhow::bail!("product abandoned registration");
            })
            .await;
        assert_eq!(
            registration.unwrap_err().to_string(),
            "product abandoned registration"
        );
        let item = register(&fixture.state, &fixture.agent, &task).await;
        assert_eq!(item.key, "FL-1");
        let location = fixture.storage.upload(&item, &[0; 101]);
        let prepared =
            prepare_agent_file_completion(&state, &fixture.agent, target(&task), &item.key)
                .await
                .unwrap();
        let refusal: anyhow::Result<()> = state
            .run_write(async |write| {
                let outcome =
                    record_completion(write, &fixture.agent, target(&task), prepared).await?;
                assert_eq!(
                    outcome.into_result().unwrap_err().code(),
                    "agent_file.too_large"
                );
                anyhow::bail!("product abandoned refusal");
            })
            .await;
        assert_eq!(
            refusal.unwrap_err().to_string(),
            "product abandoned refusal"
        );
        assert!(records.lock().unwrap().is_empty());
        assert!(fixture.storage.contains(&location, &[0; 101]));
        assert_eq!(fixture.storage.deletes(), 0);
        let error = complete_agent_file(&fixture.state, &fixture.agent, target(&task), &item.key)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "agent_file.too_large");
        assert!(!fixture.storage.contains(&location, &[0; 101]));
        assert!(fixture.read(&task).await.files.is_empty());
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
