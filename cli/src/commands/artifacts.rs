use lorebay_lib_types::Secret;
use reqwest::Method;
use typed_builder::TypedBuilder;

use crate::args::ArtifactCommand;
use crate::constants::{CLI_CONNECTION, DATA};
use crate::credentials::Store;
use crate::error::{Failure, Result};
use crate::http::Http;
use crate::output::{self, Outcome, Render};

use super::{api_path, delete};

pub(super) async fn run(command: ArtifactCommand, store: &Store) -> Result<Outcome> {
    match command {
        ArtifactCommand::Delete {
            space,
            task_id,
            artifact,
            yes,
        } => {
            delete(
                store,
                &api_path(&[SPACES, &space, TASKS, &task_id, ARTIFACTS, &artifact])?,
                &format!("artifact {artifact} of task {task_id} in {space}"),
                yes,
                "Artifact deleted.",
            )
            .await
        }
        ArtifactCommand::Url { space, artifact } => {
            let body = Http::new()?
                .authorized(
                    store,
                    Method::GET,
                    &api_path(&[SPACES, &space, ARTIFACTS, &artifact, URL])?,
                    None,
                )
                .await?;
            let url = body[DATA][URL]
                .as_str()
                .ok_or_else(|| {
                    Failure::local(
                        CLI_CONNECTION,
                        "The server returned an invalid artifact URL.",
                    )
                })?
                .to_owned();
            Ok(Outcome::new(body, DownloadUrl::builder().url(url).build()))
        }
    }
}

const ARTIFACTS: &str = "artifacts";
const SPACES: &str = "spaces";
const TASKS: &str = "tasks";
const URL: &str = "url";

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct DownloadUrl {
    url: Secret<String>,
}

impl Render for DownloadUrl {
    fn human(&self) -> String {
        output::safe(self.url.expose())
    }
}
