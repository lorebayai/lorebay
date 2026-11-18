use std::io::Error;

use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::map_response;
use axum::response::{IntoResponse, Response};
use axum::routing::put;
use futures_util::stream::try_unfold;
use lorebay_base::Secret;
use serde::Deserialize;
use thiserror::Error;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use typed_builder::TypedBuilder;

use crate::api::ProblemDetails;
use crate::domain::{ArtifactError, check_artifact_download};
use crate::server::ServerState;
use crate::storage::{FileOperation, StorageError};

use headers::download_headers;

pub(crate) fn router() -> Router<ServerState> {
    Router::new()
        .route("/files/{location}", put(upload).get(download))
        .layer(DefaultBodyLimit::disable())
        .layer(map_response(secure_response))
}

#[derive(Debug, Error)]
enum FileError {
    #[error("Content-Encoding is not supported for uploads")]
    Encoding,
    #[error("The upload requires If-None-Match: *")]
    Header,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("The declared content length is invalid")]
    Length,
    #[error(transparent)]
    Storage(#[from] StorageError),
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
struct SignatureQuery {
    expires: i64,
    signature: Secret<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
struct DownloadQuery {
    expires: i64,
    signature: Secret<String>,
    file_name: String,
}

impl IntoResponse for FileError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::Encoding => (StatusCode::UNSUPPORTED_MEDIA_TYPE, "file.content_encoding"),
            Self::Internal(error) => {
                tracing::error!(%error, "artifact download failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "server.internal")
            }
            Self::Header => (StatusCode::FORBIDDEN, "file.headers_required"),
            Self::Length => (StatusCode::BAD_REQUEST, "file.invalid_length"),
            Self::Storage(error) => match error {
                StorageError::AlreadyExists => {
                    (StatusCode::PRECONDITION_FAILED, "file.already_exists")
                }
                StorageError::Interrupted => (StatusCode::BAD_REQUEST, "file.interrupted"),
                StorageError::Io(error) => {
                    tracing::error!(%error, "artifact file operation failed");
                    (StatusCode::INTERNAL_SERVER_ERROR, "server.internal")
                }
                StorageError::NotFound => (StatusCode::NOT_FOUND, "file.not_found"),
                StorageError::TooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "file.too_large"),
                StorageError::TooMany => (StatusCode::SERVICE_UNAVAILABLE, "file.upload_limit"),
                StorageError::UrlExpired => (StatusCode::FORBIDDEN, "file.url_expired"),
                StorageError::UrlInvalid => (StatusCode::FORBIDDEN, "file.url_invalid"),
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

impl From<ArtifactError> for FileError {
    fn from(error: ArtifactError) -> Self {
        match error {
            ArtifactError::NotFound => Self::Storage(StorageError::NotFound),
            error => Self::Internal(error.into()),
        }
    }
}

mod headers;

async fn download(
    State(state): State<ServerState>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<DownloadQuery>, QueryRejection>,
) -> Result<Response, FileError> {
    let Path(location) = path.map_err(|_| StorageError::UrlInvalid)?;
    let Query(query) = query.map_err(|_| StorageError::UrlInvalid)?;
    state.storage.authorize(
        FileOperation::Download(&query.file_name),
        &location,
        query.expires,
        query.signature.expose(),
    )?;
    check_artifact_download(&state.database, &location).await?;
    let (file, size) = state.storage.open_download(&location).await?;
    let headers = download_headers(&query.file_name, size).map_err(anyhow::Error::from)?;
    Ok((headers, Body::from_stream(try_unfold(file, file_chunk))).into_response())
}

async fn file_chunk(mut file: File) -> Result<Option<(Vec<u8>, File)>, Error> {
    let mut bytes = vec![0; DOWNLOAD_CHUNK_BYTES];
    let length = file.read(&mut bytes).await?;
    if length == 0 {
        return Ok(None);
    }
    bytes.truncate(length);
    Ok(Some((bytes, file)))
}

async fn secure_response(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    if headers
        .get(header::CONTENT_TYPE)
        .is_none_or(|value| value != "application/pdf")
    {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("sandbox"),
        );
    }
    response
}

const DOWNLOAD_CHUNK_BYTES: usize = 64 * 1024;

async fn upload(
    State(state): State<ServerState>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<SignatureQuery>, QueryRejection>,
    request: Request,
) -> Result<StatusCode, FileError> {
    let Path(location) = path.map_err(|_| StorageError::UrlInvalid)?;
    let Query(query) = query.map_err(|_| StorageError::UrlInvalid)?;
    state.storage.authorize(
        FileOperation::Upload,
        &location,
        query.expires,
        query.signature.expose(),
    )?;
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
            return Err(StorageError::TooLarge.into());
        }
    }
    Ok(())
}
