use reqwest::Method;
use typed_builder::TypedBuilder;
use url::{Position, Url};

use crate::args::{OutputMode, Page};
use crate::constants::{CLI_CONNECTION, CLI_USAGE, DATA, HUMAN_OUTPUT, JSON_OUTPUT, META};
use crate::credentials::Store;
use crate::error::{Failure, Result};
use crate::http::Http;
use crate::output::{self, Outcome, Table};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Listing {
    pub path: String,
    pub command: String,
    pub columns: Vec<&'static str>,
    pub fields: Vec<&'static str>,
}

pub(crate) fn api_path(segments: &[&str]) -> Result<String> {
    let mut url = Url::parse(API_ROOT)
        .map_err(|_| Failure::local(CLI_USAGE, "Could not construct the API path."))?;
    url.path_segments_mut()
        .map_err(|_| Failure::local(CLI_USAGE, "Invalid API path."))?
        .pop_if_empty()
        .extend(segments);
    Ok(url.path().to_owned())
}

pub(crate) async fn list_page(
    store: &Store,
    page: Page,
    mode: OutputMode,
    listing: Listing,
) -> Result<Outcome> {
    let mut url = Url::parse(API_ROOT)
        .map_err(|_| Failure::local(CLI_USAGE, "Could not construct the list path."))?;
    url.set_path(&listing.path);
    if let Some(size) = page.page_size {
        url.query_pairs_mut()
            .append_pair("page_size", &size.to_string());
    }
    if let Some(cursor) = &page.cursor {
        url.query_pairs_mut().append_pair("cursor", cursor);
    }
    let body = Http::new()?
        .authorized(store, Method::GET, &url[Position::BeforePath..], None)
        .await?;
    let items = body[DATA]
        .as_array()
        .ok_or_else(|| Failure::local(CLI_CONNECTION, "The server returned an invalid list."))?;
    let rows: Vec<_> = items
        .iter()
        .map(|item| {
            listing
                .fields
                .iter()
                .map(|field| item[field].as_str().unwrap_or_default().to_owned())
                .collect::<Vec<_>>()
        })
        .collect();
    let next = body[META]["next_cursor"]
        .as_str()
        .filter(|cursor| !cursor.is_empty())
        .map(|cursor| next_page(&listing.command, &page, cursor, mode));
    let mut outcome = Outcome::new(
        body,
        Table::builder().columns(listing.columns).rows(rows).build(),
    );
    outcome.next = next;
    Ok(outcome)
}

const API_ROOT: &str = "http://localhost/api/v1/";

fn next_page(command: &str, page: &Page, cursor: &str, mode: OutputMode) -> String {
    let size = page
        .page_size
        .map(|size| format!(" --page-size {size}"))
        .unwrap_or_default();
    let format = match mode {
        OutputMode::Human => HUMAN_OUTPUT,
        OutputMode::Json => JSON_OUTPUT,
    };
    format!(
        "Next page: lorebay {command}{size} --cursor {} --output {format}",
        output::shell_word(cursor)
    )
}
