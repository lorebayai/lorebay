use axum::Json;
use axum::Router;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::put;
use lorebay_base::Secret;
use serde::Deserialize;
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::api::ProblemDetails;
use crate::server::ServerState;
use crate::storage::UploadError;

pub(crate) fn router() -> Router<ServerState> {
    Router::new()
        .route("/files/{location}", put(upload))
        .layer(DefaultBodyLimit::disable())
}

#[derive(Debug, Error)]
enum FileError {
    #[error("Content-Encoding is not supported for uploads")]
    Encoding,
    #[error("The upload requires If-None-Match: *")]
    Header,
    #[error("The declared content length is invalid")]
    Length,
    #[error(transparent)]
    Upload(#[from] UploadError),
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
struct SignatureQuery {
    expires: i64,
    signature: Secret<String>,
}

impl IntoResponse for FileError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::Encoding => (StatusCode::UNSUPPORTED_MEDIA_TYPE, "file.content_encoding"),
            Self::Header => (StatusCode::FORBIDDEN, "file.headers_required"),
            Self::Length => (StatusCode::BAD_REQUEST, "file.invalid_length"),
            Self::Upload(error) => match error {
                UploadError::AlreadyExists => {
                    (StatusCode::PRECONDITION_FAILED, "file.already_exists")
                }
                UploadError::Interrupted => (StatusCode::BAD_REQUEST, "file.interrupted"),
                UploadError::Io(error) => {
                    tracing::error!(%error, "artifact upload failed");
                    (StatusCode::INTERNAL_SERVER_ERROR, "server.internal")
                }
                UploadError::TooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "file.too_large"),
                UploadError::TooMany => (StatusCode::SERVICE_UNAVAILABLE, "file.upload_limit"),
                UploadError::UrlExpired => (StatusCode::FORBIDDEN, "file.url_expired"),
                UploadError::UrlInvalid => (StatusCode::FORBIDDEN, "file.url_invalid"),
            },
        };
        let title = if status == StatusCode::INTERNAL_SERVER_ERROR {
            "Internal Server Error".to_owned()
        } else {
            self.to_string()
        };
        let problem = ProblemDetails::builder()
            .problem_type("about:blank")
            .title(title)
            .status(status.as_u16())
            .code(code)
            .errors(Vec::new())
            .build();
        let mut response = (
            status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(problem),
        )
            .into_response();
        if status == StatusCode::SERVICE_UNAVAILABLE {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
        }
        response
    }
}

async fn upload(
    State(state): State<ServerState>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<SignatureQuery>, QueryRejection>,
    request: Request,
) -> Result<StatusCode, FileError> {
    let Path(location) = path.map_err(|_| UploadError::UrlInvalid)?;
    let Query(query) = query.map_err(|_| UploadError::UrlInvalid)?;
    state
        .storage
        .authorize(&location, query.expires, query.signature.expose())?;
    validate_headers(request.headers(), state.storage.size_limit)?;
    let _permit = state.storage.admit()?;
    state
        .storage
        .receive(&location, request.into_body().into_data_stream())
        .await?;
    Ok(StatusCode::CREATED)
}

fn validate_headers(headers: &HeaderMap, size_limit: u64) -> Result<(), FileError> {
    if headers.get_all(header::IF_NONE_MATCH).iter().count() != 1
        || headers
            .get(header::IF_NONE_MATCH)
            .is_none_or(|value| value != "*")
    {
        return Err(FileError::Header);
    }
    if headers.contains_key(header::CONTENT_ENCODING) {
        return Err(FileError::Encoding);
    }
    if let Some(value) = headers.get(header::CONTENT_LENGTH) {
        let length = value
            .to_str()
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or(FileError::Length)?;
        if length > size_limit {
            return Err(UploadError::TooLarge.into());
        }
    }
    Ok(())
}
