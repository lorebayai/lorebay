use axum::Json;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use typed_builder::TypedBuilder;

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Envelope<T> {
    pub data: T,
    #[builder(default = Meta::default())]
    pub meta: Meta,
}

#[derive(Clone, Debug, Default, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Meta {
    #[builder(default = None)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<Pagination>,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Pagination {
    pub next_cursor: Option<String>,
}

impl<T: Serialize> IntoResponse for Envelope<T> {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}
