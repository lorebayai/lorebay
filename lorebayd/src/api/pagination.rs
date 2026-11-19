use serde::Deserialize;
use typed_builder::TypedBuilder;

use crate::domain::pagination::PageOptions;
use lorebay_lib_core::PageSizeError;

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
        Self::from_text(query.page_size.as_deref(), query.cursor)
            .map_err(|_| invalid_query(PageSizeError::FIELD))
    }
}

pub(crate) fn invalid_query(field: &str) -> ApiError {
    ApiError::InvalidQuery(vec![
        FieldError::builder()
            .field(field)
            .code(PageSizeError::FIELD_CODE)
            .message(PageSizeError.to_string())
            .build(),
    ])
}
