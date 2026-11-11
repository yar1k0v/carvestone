//! `GET /api/events`: server-sent events for what changed on disk.
//!
//! The watcher and the write routes publish to one broadcast bus; every connected client holds a receiver, so a
//! client that disconnects (or lags) never holds back the others.

use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use carvestone_engine::Repo;
use carvestone_engine::api::{CodeEvent, ErrorEvent, ItemsEvent};
use carvestone_engine::changes::ids_for_paths;
use carvestone_engine::code_native::NativeStatus;
use carvestone_engine::query::etag_of;
use futures_util::Stream;
use futures_util::stream;
use serde::Serialize;
use tokio::sync::broadcast::{self, error::RecvError};

use crate::AppState;

/// Events a slow client may fall behind by before it is told it missed some.
const BACKLOG: usize = 64;
/// A comment line this often keeps idle connections open through proxies.
const PING: Duration = Duration::from_secs(15);
/// How long the watcher may take to see a write the server made; later it is forgotten.
const OWN_WRITE_TTL: Duration = Duration::from_secs(10);

/// What the watcher found after one debounced batch.
#[derive(Debug, Clone)]
pub(crate) enum Change {
    Items(ItemsEvent),
    Code(CodeEvent),
    Error(ErrorEvent),
    /// A step of the native pass the UI started, with the whole status after it.
    Native(NativeStatus),
}

/// A file this server wrote, remembered so the watcher does not announce it a second time.
struct OwnWrite {
    path: String,
    etag: String,
    at: Instant,
}

#[derive(Clone)]
pub(crate) struct Bus {
    tx: broadcast::Sender<Change>,
    own: Arc<Mutex<Vec<OwnWrite>>>,
}

impl Bus {
    pub(crate) fn new() -> Bus {
        Bus {
            tx: broadcast::channel(BACKLOG).0,
            own: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Delivers to every connected client; with none connected the change is dropped.
    pub(crate) fn publish(&self, change: Change) {
        let _ = self.tx.send(change);
    }

    /// Announces a file the server just wrote (`path` relative to the repo, `text` as saved), and
    /// marks it so the watcher stays silent about the same bytes. Works without a watcher.
    pub(crate) fn publish_write(&self, repo: &Repo, path: &str, text: &str) {
        let ids = ids_for_paths(repo, &[path]);
        if ids.is_empty() {
            return;
        }
        if let Ok(mut own) = self.own.lock() {
            own.retain(|w| w.at.elapsed() < OWN_WRITE_TTL);
            own.push(OwnWrite {
                path: path.to_string(),
                etag: etag_of(text),
                at: Instant::now(),
            });
        }
        self.publish(Change::Items(ItemsEvent { ids }));
    }

    /// True once per recorded write when `path` holds exactly the bytes the server wrote. Another
    /// process changing the file afterwards has other bytes and is still reported.
    pub(crate) fn take_own_write(&self, path: &str, etag: &str) -> bool {
        let Ok(mut own) = self.own.lock() else {
            return false;
        };
        own.retain(|w| w.at.elapsed() < OWN_WRITE_TTL);
        match own.iter().position(|w| w.path == path && w.etag == etag) {
            Some(at) => {
                own.remove(at);
                true
            }
            None => false,
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<Change> {
        self.tx.subscribe()
    }
}

fn frame<T: Serialize>(name: &str, payload: &T) -> Event {
    Event::default()
        .event(name)
        .json_data(payload)
        .unwrap_or_else(|_| Event::default().comment("event could not be encoded"))
}

fn event_of(change: &Change) -> Event {
    match change {
        Change::Items(payload) => frame("items", payload),
        Change::Code(payload) => frame("code", payload),
        Change::Error(payload) => frame("error", payload),
        Change::Native(payload) => frame("native", payload),
    }
}

pub(crate) async fn events(State(state): State<AppState>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.bus.subscribe();
    let stream = stream::unfold(rx, |mut rx| async move {
        let event = match rx.recv().await {
            Ok(change) => event_of(&change),
            // The client cannot know which changes it lost, so it refetches everything.
            Err(RecvError::Lagged(_)) => Event::default().event("lagged").data("{}"),
            Err(RecvError::Closed) => return None,
        };
        Some((Ok(event), rx))
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(PING).text("ping"))
}
