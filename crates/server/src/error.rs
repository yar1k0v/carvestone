//! Errors on the wire: `{error, kind}` with a status per kind.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use carvestone_engine::api::ErrorBody;
use carvestone_engine::{EngineError, ErrorKind};

/// A failed request: the JSON body and the status that goes with its kind.
#[derive(Debug, Clone)]
pub struct ApiError {
    pub body: ErrorBody,
    status: StatusCode,
}

fn status_of(kind: ErrorKind) -> StatusCode {
    match kind {
        ErrorKind::NotFound => StatusCode::NOT_FOUND,
        ErrorKind::Invalid => StatusCode::UNPROCESSABLE_ENTITY,
        ErrorKind::Conflict => StatusCode::CONFLICT,
        ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
        ErrorKind::Forbidden => StatusCode::FORBIDDEN,
        ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

impl ApiError {
    pub fn new(kind: ErrorKind, error: impl Into<String>) -> ApiError {
        ApiError {
            body: ErrorBody {
                error: error.into(),
                kind: kind.as_str().to_string(),
            },
            status: status_of(kind),
        }
    }

    pub fn bad_request(error: impl Into<String>) -> ApiError {
        ApiError::new(ErrorKind::BadRequest, error)
    }

    pub fn not_found(error: impl Into<String>) -> ApiError {
        ApiError::new(ErrorKind::NotFound, error)
    }

    pub fn forbidden(error: impl Into<String>) -> ApiError {
        ApiError::new(ErrorKind::Forbidden, error)
    }

    pub fn internal(error: impl Into<String>) -> ApiError {
        ApiError::new(ErrorKind::Internal, error)
    }
}

impl From<EngineError> for ApiError {
    fn from(err: EngineError) -> ApiError {
        ApiError::new(err.kind(), err.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}
