use crate::args::CheckpointCommand;
use crate::credentials::Store;
use crate::error::Result;
use crate::output::Outcome;
use crate::time;

use super::{api_path, delete};

pub(super) async fn run(command: CheckpointCommand, store: &Store) -> Result<Outcome> {
    match command {
        CheckpointCommand::Delete {
            space,
            task_id,
            checkpoint,
            yes,
        } => {
            let timestamp = time::parse(&checkpoint).unwrap_or_else(|| checkpoint.clone());
            delete(
                store,
                &api_path(&[SPACES, &space, TASKS, &task_id, CHECKPOINTS, &timestamp])?,
                &format!("checkpoint {checkpoint} of task {task_id} in {space}"),
                yes,
                "Checkpoint deleted.",
            )
            .await
        }
    }
}

const CHECKPOINTS: &str = "checkpoints";
const SPACES: &str = "spaces";
const TASKS: &str = "tasks";
