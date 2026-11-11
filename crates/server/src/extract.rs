//! Request parsing that fails as `{error, kind: "bad_request"}` instead of axum's plain text, and
//! the helper that runs blocking engine calls off the async threads.

use axum::body::to_bytes;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use carvestone_engine::{EngineError, Repo};
use serde::de::DeserializeOwned;

use crate::AppState;
use crate::error::ApiError;

/// Bodies larger than this are refused; a section of text is far smaller.
const BODY_LIMIT: usize = 4 * 1024 * 1024;

/// Query-string parameters.
pub(crate) struct Q<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequestParts<S> for Q<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, ApiError> {
        axum::extract::Query::<T>::try_from_uri(&parts.uri)
            .map(|query| Q(query.0))
            .map_err(|err| ApiError::bad_request(err.to_string()))
    }
}

/// A JSON request body.
pub(crate) struct Body<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for Body<T> {
    type Rejection = ApiError;

    async fn from_request(req: Request, _: &S) -> Result<Self, ApiError> {
        let bytes = to_bytes(req.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| ApiError::bad_request("the request body is too large or could not be read"))?;
        serde_json::from_slice(&bytes)
            .map(Body)
            .map_err(|err| ApiError::bad_request(format!("invalid JSON body: {err}")))
    }
}

/// Runs a blocking engine call on its own thread; the engine reads files and takes locks.
pub(crate) async fn run<T, F>(state: &AppState, call: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(Repo) -> Result<T, EngineError> + Send + 'static,
{
    let repo = state.repo.clone();
    tokio::task::spawn_blocking(move || call(repo))
        .await
        .map_err(|err| ApiError::internal(format!("the request failed: {err}")))?
        .map_err(ApiError::from)
}
