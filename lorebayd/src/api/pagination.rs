use serde::Deserialize;
use typed_builder::TypedBuilder;

use crate::domain::pagination::PageOptions;
use lorebay_lib_core::DEFAULT_PAGE_SIZE;

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
        let options = Self {
            page_size,
            cursor: query.cursor,
        };
        options.validate().map_err(|_| invalid_query("page_size"))?;
        Ok(options)
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
