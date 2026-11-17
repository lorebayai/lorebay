use serde::Deserialize;
use typed_builder::TypedBuilder;

use crate::domain::pagination::PageOptions;

use super::{ApiError, FieldError};

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ListQuery {
    page_size: Option<String>,
    cursor: Option<String>,
}

impl TryFrom<ListQuery> for PageOptions {
    type Error = ApiError;

    fn try_from(query: ListQuery) -> Result<Self, Self::Error> {
        let page_size = query
            .page_size
            .as_deref()
            .map(str::parse::<u64>)
            .transpose()
            .map_err(|_| invalid_query("page_size"))?
            .unwrap_or(DEFAULT_PAGE_SIZE);
        if !(1..=MAX_PAGE_SIZE).contains(&page_size) {
            return Err(invalid_query("page_size"));
        }
        Ok(Self::builder()
            .page_size(page_size)
            .cursor(query.cursor)
            .build())
    }
}

pub(crate) fn invalid_query(field: &str) -> ApiError {
    ApiError::InvalidQuery(vec![
        FieldError::builder()
            .field(field)
            .code("invalid")
            .message("Invalid pagination parameter.")
            .build(),
    ])
}

const DEFAULT_PAGE_SIZE: u64 = 20;
const MAX_PAGE_SIZE: u64 = 100;
