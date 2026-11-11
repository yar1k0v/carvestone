//! `/api/code/native`: GET is the current or last pass, POST starts one in the background.
//!
//! One pass runs at a time. Each step updates the shared status and goes out as `event: native`
//! with the whole status, so a page opened mid-pass catches up from GET and then follows events.

use std::sync::{Arc, Mutex, MutexGuard};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use carvestone_engine::ErrorKind;
use carvestone_engine::api::CodeEvent;
use carvestone_engine::code_native::{NativeQuery, NativeStatus, native_with};

use crate::AppState;
use crate::error::ApiError;
use crate::events::Change;

/// The status every request and the running pass share.
#[derive(Clone, Default)]
pub(crate) struct Native(Arc<Mutex<NativeStatus>>);

impl Native {
    fn lock(&self) -> MutexGuard<'_, NativeStatus> {
        // A pass that panicked leaves a status worth showing; take it as it is.
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Changes the status and returns a copy to publish.
    fn update(&self, change: impl FnOnce(&mut NativeStatus)) -> NativeStatus {
        let mut status = self.lock();
        change(&mut status);
        status.clone()
    }
}

pub(crate) async fn status(State(state): State<AppState>) -> Json<NativeStatus> {
    Json(state.native.lock().clone())
}

pub(crate) async fn start(State(state): State<AppState>) -> Result<(StatusCode, Json<NativeStatus>), ApiError> {
    let first = {
        let mut status = state.native.lock();
        if status.running {
            return Err(ApiError::new(
                ErrorKind::Conflict,
                "code links are already being made exact; wait for that pass to end",
            ));
        }
        status.start();
        status.clone()
    };
    state.bus.publish(Change::Native(first.clone()));

    let pass = state.clone();
    let job = tokio::task::spawn_blocking(move || {
        let result = native_with(&pass.repo, &NativeQuery::default(), &mut |step| {
            let now = pass.native.update(|status| status.push(step));
            pass.bus.publish(Change::Native(now));
        });
        result.map_err(|err| err.to_string())
    });
    let done = state.clone();
    tokio::spawn(async move {
        let result = job
            .await
            .unwrap_or_else(|err| Err(format!("the pass stopped unexpectedly: {err}")));
        let ok = result.is_ok();
        let last = done.native.update(|status| status.finish(result));
        done.bus.publish(Change::Native(last));
        if ok {
            // Code answers may have changed; the paths are unknown, so every code query refetches.
            if let Some(gate) = done.repo.freshness() {
                gate.touch();
            }
            done.bus.publish(Change::Code(CodeEvent { paths: Vec::new() }));
        }
    });
    Ok((StatusCode::ACCEPTED, Json(first)))
}
