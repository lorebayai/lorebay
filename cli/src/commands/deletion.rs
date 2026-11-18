use reqwest::Method;

use crate::credentials::Store;
use crate::error::Result;
use crate::http::Http;
use crate::output::{self, Outcome};

pub(crate) async fn delete(
    store: &Store,
    path: &str,
    target: &str,
    yes: bool,
    message: &'static str,
) -> Result<Outcome> {
    if !yes {
        output::confirm("Delete", target)?;
    }
    let body = Http::new()?
        .authorized(store, Method::DELETE, path, None)
        .await?;
    let mut outcome = Outcome::completion(message);
    outcome.body = body;
    Ok(outcome)
}
