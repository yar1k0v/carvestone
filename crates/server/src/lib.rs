//! carvestone-server: the engine over HTTP for the web UI.
//!
//! Handlers parse a request, call one engine function and return its struct as JSON; no
//! behaviour lives here. The server binds loopback by default and rejects foreign `Host`
//! headers and cross-site writes (see `guard`).

mod cache;
mod error;
mod events;
mod extract;
mod guard;
mod native;
mod routes;
mod ui;
mod watch;
mod work;
mod write;

use std::io;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use carvestone_engine::Repo;
use carvestone_engine::design::hld;
use carvestone_engine::fresh::Freshness;
use carvestone_engine::modules::modules;
use carvestone_engine::product::product;
use tokio::net::TcpListener;
use tower::ServiceExt;

use guard::AllowedHosts;

pub use error::ApiError;
pub use watch::Watcher;

/// Why `serve` stopped or never started.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("port {0} is in use; pass --port")]
    PortInUse(u16),
    #[error("cannot listen on {addr}: {source}")]
    Bind { addr: String, source: io::Error },
    #[error("cannot watch files: {0}; pass --no-watch to run without")]
    Watch(#[from] notify::Error),
    #[error("the server stopped: {0}")]
    Io(#[from] io::Error),
}

/// What `stone serve` was asked for.
#[derive(Debug, Clone)]
pub struct ServeOpts {
    pub host: String,
    pub port: u16,
    /// Extra `Host` names to accept, besides loopback and the bound address (`--allow-host`).
    pub allow_host: Vec<String>,
    /// Watch `stone/` and the code roots and push events (`--no-watch` turns it off).
    pub watch: bool,
    /// Open the page in the default browser.
    pub open: bool,
}

impl Default for ServeOpts {
    fn default() -> Self {
        ServeOpts {
            host: "127.0.0.1".to_string(),
            port: 4747,
            allow_host: Vec::new(),
            watch: true,
            open: false,
        }
    }
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub repo: Repo,
    /// Which `Host` values and `Origin`s a request may name.
    pub hosts: Arc<AllowedHosts>,
    /// What `/api/events` clients hear; the watcher and the write routes publish to it.
    pub bus: events::Bus,
    /// The native pass the UI started, running or last finished.
    pub native: native::Native,
    /// Remembered read answers; `None` computes every request.
    pub cache: Option<cache::Cache>,
}

/// Modules whose explain is computed ahead of the first visit.
const WARM_MODULES: usize = 24;
/// How late a change nothing reports (a new commit) can show up while a watcher runs.
const WATCHED_AGE: Duration = Duration::from_secs(30);
/// The same without a watcher, when this age is all that notices a changed file.
const UNWATCHED_AGE: Duration = Duration::from_secs(2);

fn app(state: AppState) -> Router {
    Router::new()
        .route("/api/items", get(routes::list_items).post(write::create_item))
        .route("/api/items/{*id}", get(routes::get_item).patch(write::patch_item))
        .route("/api/search", get(routes::search_items))
        .route("/api/code/native", get(native::status).post(native::start))
        .route("/api/code/domain", get(routes::code_domain))
        .route("/api/code/{op}", get(routes::code_op))
        .route("/api/why", get(routes::why))
        .route("/api/history/{*id}", get(routes::history))
        .route("/api/timeline", get(routes::timeline))
        .route("/api/config", get(routes::config))
        .route("/api/docs", get(routes::docs))
        .route("/api/product", get(work::product))
        .route("/api/features/{*id}", get(work::feature))
        .route("/api/activity", get(work::activity))
        .route("/api/tasks/{*path}", get(work::task_answer))
        .route("/api/arch/graph", get(routes::arch))
        .route("/api/arch/hld", get(routes::arch_hld))
        .route("/api/arch/lld/{*id}", get(routes::arch_lld))
        .route("/api/arch/c4/context", get(routes::arch_c4_context))
        .route("/api/arch/c4/container", get(routes::arch_c4_container))
        .route("/api/arch/c4/component/{*id}", get(routes::arch_c4_component))
        .route("/api/events", get(events::events))
        .fallback(routes::fallback)
        .layer(from_fn_with_state(state.clone(), cache::cached))
        .layer(from_fn_with_state(state.clone(), guard::guard))
        .with_state(state)
}

fn local_state(repo: Repo) -> AppState {
    AppState {
        repo,
        hosts: Arc::new(AllowedHosts::default()),
        bus: events::Bus::new(),
        native: native::Native::default(),
        cache: None,
    }
}

/// `state` as a long-running server keeps it: reads share refreshes and repeated reads are remembered.
fn lasting(state: AppState, max_age: Duration, watched: bool) -> AppState {
    AppState {
        repo: state.repo.clone().with_freshness(Freshness::new(max_age, watched)),
        cache: Some(cache::Cache::default()),
        ..state
    }
}

/// `router` as `stone serve` runs it, remembering answers; nothing reports changes but the
/// repo's own `freshness`, which the caller drives. Tests use it.
pub fn lasting_router(repo: Repo) -> Router {
    lasting_router_with(repo).0
}

/// `lasting_router` and the gate that tells it the tree moved.
pub fn lasting_router_with(repo: Repo) -> (Router, Arc<Freshness>) {
    let state = lasting(local_state(repo), WATCHED_AGE, true);
    let gate = state.repo.freshness().cloned().expect("lasting sets it");
    (app(state), gate)
}

/// `text` as the web client writes a query value, so a warmed answer is found under the same URL.
fn query_value(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => char::from(byte).to_string(),
            b' ' => "+".to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// Computes what a first visit asks for, so the pages open from memory. Best effort: a failure
/// here is the page's to report when someone opens it.
async fn warm(app: Router, repo: Repo) {
    let design_repo = repo.clone();
    let Ok(Ok(design)) = tokio::task::spawn_blocking(move || hld(&design_repo)).await else {
        return;
    };
    let mut urls: Vec<String> = [
        "/api/config",
        "/api/items",
        "/api/docs",
        "/api/product",
        // The first search opens the search index; a word nobody typed keeps it out of the way.
        "/api/search?q=warm&type=doc&limit=1",
        "/api/activity?period=week",
        "/api/arch/hld",
        "/api/arch/graph",
        "/api/code/domain",
    ]
    .map(String::from)
    .to_vec();
    urls.extend(design.containers.iter().flat_map(|part| {
        let id = part.id.replace('/', "%2F");
        [
            format!("/api/arch/lld/{}?service=false", part.id),
            format!("/api/code/explain?target={id}"),
            format!("/api/code/domain?part={id}"),
        ]
    }));
    let product_repo = repo.clone();
    if let Ok(Ok(found)) = tokio::task::spawn_blocking(move || product(&product_repo)).await {
        urls.extend(
            found
                .features
                .iter()
                .map(|feature| format!("/api/features/{}", feature.id)),
        );
    }
    // The biggest modules first; a project can have hundreds, each a few seconds of work.
    let shared = repo.clone();
    if let Ok(Ok(mut found)) = tokio::task::spawn_blocking(move || modules(&shared)).await {
        found.sort_by_key(|module| std::cmp::Reverse(module.files.len()));
        urls.extend(
            found
                .iter()
                .take(WARM_MODULES)
                .map(|module| format!("/api/code/explain?target={}", query_value(&module.id))),
        );
    }
    for url in urls {
        let Ok(request) = Request::builder()
            .uri(url)
            .header(header::HOST, "127.0.0.1")
            .body(Body::empty())
        else {
            continue;
        };
        let _ = app.clone().oneshot(request).await;
    }
}

/// The API for `repo`, answering `localhost` and loopback hosts, without a file watcher (so
/// `/api/events` stays silent). Tests drive it with `oneshot`.
pub fn router(repo: Repo) -> Router {
    app(local_state(repo))
}

/// `router` plus a file watcher that feeds `/api/events`; the watcher runs while the guard lives.
pub fn watched_router(repo: Repo) -> Result<(Router, Watcher), ServerError> {
    let state = local_state(repo);
    let watcher = watch::start(state.repo.clone(), state.bus.clone())?;
    Ok((app(state), watcher))
}

/// Binds, prints the URL and serves until Ctrl+C.
pub async fn serve(repo: Repo, opts: ServeOpts) -> Result<(), ServerError> {
    let listener = TcpListener::bind((opts.host.as_str(), opts.port))
        .await
        .map_err(|source| match source.kind() {
            io::ErrorKind::AddrInUse => ServerError::PortInUse(opts.port),
            _ => ServerError::Bind {
                addr: format!("{}:{}", opts.host, opts.port),
                source,
            },
        })?;
    let local = listener.local_addr()?;
    let max_age = if opts.watch { WATCHED_AGE } else { UNWATCHED_AGE };
    let state = lasting(
        AppState {
            hosts: Arc::new(AllowedHosts::for_bind(local.ip(), &opts.host, &opts.allow_host)),
            ..local_state(repo)
        },
        max_age,
        opts.watch,
    );
    // Held until the server stops; dropping it ends the watcher.
    let _watcher = if opts.watch {
        Some(watch::start(state.repo.clone(), state.bus.clone())?)
    } else {
        None
    };
    let url = format!("http://{local}");
    println!("stone serve: {url}");
    if !local.ip().is_loopback() {
        eprintln!("warning: no authentication; anyone on this network can edit");
    }
    if opts.open {
        open_browser(&url);
    }
    let repo = state.repo.clone();
    let router = app(state);
    tokio::spawn(warm(router.clone(), repo));
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

/// Best effort: a missing browser launcher is not worth failing the server for.
fn open_browser(url: &str) {
    use std::process::Command;
    let started = if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", "start", "", url]).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };
    if started.is_err() {
        eprintln!("cannot open a browser; open {url} yourself");
    }
}
