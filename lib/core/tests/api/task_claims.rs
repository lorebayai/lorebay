use lorebay_lib_core::{claim_task, plan_task};
use tokio::sync::mpsc;

use super::helpers::{Gates, Pause, SPACE, next_pid, target, with_postgres};

#[tokio::test]
async fn claim_task_allows_exactly_one_concurrent_claim_of_planned_task() {
    with_postgres(async |fixture| {
        fixture.space().await;
        let title = "Queued work".to_owned().try_into().unwrap();
        let planned = fixture
            .state
            .run_write(async |write| plan_task(write, &fixture.person, SPACE, title).await)
            .await
            .unwrap();
        let held = Pause::default();
        let first = fixture.state_with(
            Gates {
                before_commit: Some(held.gate()),
                ..Gates::default()
            },
            false,
        );
        let task = planned.clone();
        let caller = fixture.agent.clone();
        let winner = tokio::spawn(async move {
            first
                .run_write(async |write| claim_task(write, &caller, target(&task)).await)
                .await
        });
        held.reached().await;
        let (report_pid, mut pid) = mpsc::unbounded_channel();
        let second = fixture.state_with(
            Gates {
                report_pid: Some(report_pid),
                ..Gates::default()
            },
            false,
        );
        let task = planned.clone();
        let mut caller = fixture.agent.clone();
        caller.agent_name = Some("other worker".to_owned());
        let loser = tokio::spawn(async move {
            second
                .run_write(async |write| claim_task(write, &caller, target(&task)).await)
                .await
        });
        fixture.wait_for_lock(next_pid(&mut pid).await).await;
        held.release();
        let winner = winner.await.unwrap().unwrap();
        let loser = loser.await.unwrap().unwrap_err();
        assert_eq!(loser.code(), "task.held");
        assert!(
            loser
                .to_string()
                .contains(winner.task.holder.as_deref().unwrap())
        );
        let stored = fixture.read(&planned).await;
        assert_eq!(stored.task.holder, winner.task.holder);
        assert_eq!(stored.task.person, winner.task.person);
        assert_eq!(stored.task.planner.as_deref(), Some("alice"));
    })
    .await;
}
