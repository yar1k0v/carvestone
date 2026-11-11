//! Remembers read answers while nothing has moved.
//!
//! Every read is a pure function of the store and the index, so its bytes hold until the repo's
//! `Freshness` generation moves. Concurrent requests for one URL share a single computation, and
//! the computation finishes even when the page that asked has gone: its answer is what the next
//! visit reads. Without this, leaving a slow page left its work running with nobody to receive it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use axum::body::{Body, Bytes, to_bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use carvestone_engine::refresh::refresh;
use tokio::sync::OnceCell;

use crate::AppState;

/// Answers kept at most; a cache this full is emptied, since old URLs are as good to lose as any.
const MAX_ENTRIES: usize = 512;

#[derive(Clone)]
struct Stored {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl Stored {
    fn response(&self) -> Response {
        let mut response = (self.status, self.body.clone()).into_response();
        *response.headers_mut() = self.headers.clone();
        response
    }
}

type Slot = Arc<OnceCell<Stored>>;

#[derive(Default)]
struct Slots {
    generation: u64,
    answers: HashMap<String, Slot>,
}

/// The answers of one generation, by URL.
#[derive(Clone, Default)]
pub(crate) struct Cache(Arc<Mutex<Slots>>);

impl Cache {
    fn slots(&self) -> MutexGuard<'_, Slots> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn slot(&self, url: &str, generation: u64) -> Slot {
        let mut slots = self.slots();
        if slots.generation != generation || slots.answers.len() >= MAX_ENTRIES {
            slots.answers.clear();
            slots.generation = generation;
        }
        Arc::clone(slots.answers.entry(url.to_string()).or_default())
    }

    fn forget(&self, url: &str) {
        self.slots().answers.remove(url);
    }
}

/// The routes whose answers depend only on the store and the index. The events stream and the
/// status of a native pass change without either.
fn cacheable(path: &str) -> bool {
    let read = [
        "/api/items",
        "/api/docs",
        "/api/product",
        "/api/activity",
        "/api/search",
        "/api/why",
        "/api/timeline",
        "/api/config",
    ]
    .contains(&path)
        || [
            "/api/items/",
            "/api/code/",
            "/api/history/",
            "/api/arch/",
            "/api/features/",
            "/api/tasks/",
        ]
        .iter()
        .any(|prefix| path.starts_with(prefix));
    read && path != "/api/code/native"
}

/// Serves repeated reads from memory and marks the repo moved after a write.
pub(crate) async fn cached(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let (Some(cache), Some(gate)) = (state.cache.clone(), state.repo.freshness().cloned()) else {
        return next.run(req).await;
    };
    if req.method() != Method::GET {
        let response = next.run(req).await;
        gate.invalidate();
        return response;
    }
    if !cacheable(req.uri().path()) {
        return next.run(req).await;
    }

    if !gate.is_current() {
        // A failed refresh is reported by the handler that needs the index.
        let repo = state.repo.clone();
        let _ = tokio::task::spawn_blocking(move || refresh(&repo)).await;
    }
    let url = req.uri().to_string();
    let slot = cache.slot(&url, gate.generation());
    if let Some(stored) = slot.get() {
        return stored.response();
    }

    // On its own task: a client that disconnects drops this future, not the work.
    let computing = tokio::spawn(async move {
        let stored = slot
            .get_or_init(|| async move {
                let (parts, body) = next.run(req).await.into_parts();
                let body = to_bytes(body, usize::MAX).await.unwrap_or_default();
                Stored {
                    status: parts.status,
                    headers: parts.headers,
                    body,
                }
            })
            .await
            .clone();
        // A failed read may go better next time; only answers are worth keeping.
        if !stored.status.is_success() {
            cache.forget(&url);
        }
        stored
    });
    match computing.await {
        Ok(stored) => stored.response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Body::from("the request failed")).into_response(),
    }
}
