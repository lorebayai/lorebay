use lorebay_base::Secret;
use serde_json::Value;

use crate::api::{ApiError, ArtifactCompletedResponse, ArtifactRegisteredResponse};
use crate::domain::caller::VerifiedCaller;
use crate::domain::{ArtifactTarget, complete_artifact, register_artifact};
use crate::server::ServerState;

use super::{Arguments, value};

pub(crate) async fn create(
    state: &ServerState,
    caller: &VerifiedCaller,
    arguments: Arguments,
) -> Result<Value, ApiError> {
    let task_id = Secret::from(arguments.string("task_id")?);
    let registered = register_artifact(
        &state.database,
        state.storage.as_ref(),
        caller,
        ArtifactTarget::builder()
            .path_space(None)
            .task_id(&task_id)
            .build(),
        arguments.artifact()?,
    )
    .await?;
    value(ArtifactRegisteredResponse::from(registered))
}

pub(crate) async fn complete(
    state: &ServerState,
    caller: &VerifiedCaller,
    arguments: Arguments,
) -> Result<Value, ApiError> {
    let task_id = Secret::from(arguments.string("task_id")?);
    let completed = complete_artifact(
        &state.database,
        state.storage.as_ref(),
        caller,
        ArtifactTarget::builder()
            .path_space(None)
            .task_id(&task_id)
            .build(),
        &arguments.string("key")?,
    )
    .await?;
    value(ArtifactCompletedResponse::from(completed))
}
