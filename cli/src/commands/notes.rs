use crate::args::NoteCommand;
use crate::credentials::Store;
use crate::error::Result;
use crate::output::Outcome;

use super::{api_path, delete};

pub(super) async fn run(command: NoteCommand, store: &Store) -> Result<Outcome> {
    match command {
        NoteCommand::Delete {
            space,
            task_id,
            note,
            yes,
        } => {
            delete(
                store,
                &api_path(&[SPACES, &space, TASKS, &task_id, NOTES, &note])?,
                &format!("note {note} of task {task_id} in {space}"),
                yes,
                "Note deleted.",
            )
            .await
        }
    }
}

const NOTES: &str = "notes";
const SPACES: &str = "spaces";
const TASKS: &str = "tasks";
