//! Read routes: each parses its query, calls one engine function and returns the engine's struct.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, Method, Uri, header::ETAG};
use axum::response::{IntoResponse, Response};
use carvestone_engine::EngineError;
use carvestone_engine::api_config::{ApiConfig, api_config};
use carvestone_engine::arch_c4::{C4Source, c4_component, c4_container, c4_context};
use carvestone_engine::arch_graph::{ArchGraph, arch_graph};
use carvestone_engine::code::{self, CodeQuery};
use carvestone_engine::code_domain::{self, DomainModel};
use carvestone_engine::code_graph;
use carvestone_engine::code_symbol;
use carvestone_engine::design::{Hld, Lld, hld, lld, lld_docs};
use carvestone_engine::docs_list::{self, DocRow};
use carvestone_engine::history::{History, HistoryOpts, history as engine_history};
use carvestone_engine::modules::modules;
use carvestone_engine::query::{ListFilter, ListRow};
use carvestone_engine::search::{SearchQuery, SearchResult, search};
use carvestone_engine::timeline::{Date, Event, EventKind, TimelineOpts, parse_date, timeline as engine_timeline};
use carvestone_engine::why::{Why, why as engine_why};
use serde::Deserialize;

use crate::AppState;
use crate::error::ApiError;
use crate::extract::{Q, run};
use crate::ui;

/// Hits a search returns when `limit` is absent; the CLI default.
const SEARCH_LIMIT: usize = 10;

#[derive(Deserialize)]
pub(crate) struct ListParams {
    #[serde(rename = "type")]
    item_type: Option<String>,
    status: Option<String>,
    milestone: Option<String>,
    assignee: Option<String>,
    component: Option<String>,
    stale: Option<bool>,
}

pub(crate) async fn list_items(
    State(state): State<AppState>,
    Q(p): Q<ListParams>,
) -> Result<Json<Vec<ListRow>>, ApiError> {
    let filter = ListFilter {
        item_type: p.item_type,
        status: p.status,
        milestone: p.milestone,
        assignee: p.assignee,
        component: p.component,
        stale: p.stale.unwrap_or(false),
        ..ListFilter::default()
    };
    Ok(Json(run(&state, move |repo| repo.list(&filter)).await?))
}

#[derive(Deserialize)]
pub(crate) struct GetParams {
    section: Option<String>,
}

/// `id`, or `id#section` when `?section=` is given. Both forms at once is ambiguous.
fn with_section(id: String, section: Option<String>) -> Result<String, ApiError> {
    match section {
        Some(_) if id.contains('#') => Err(ApiError::bad_request("pass either `id#anchor` or `section`, not both")),
        Some(section) => Ok(format!("{id}#{section}")),
        None => Ok(id),
    }
}

pub(crate) async fn get_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Q(p): Q<GetParams>,
) -> Result<Response, ApiError> {
    let input = with_section(id, p.section)?;
    let result = run(&state, move |repo| repo.get(&input)).await?;
    let etag = result
        .etag()
        .and_then(|etag| HeaderValue::from_str(&format!("\"{etag}\"")).ok());
    let mut response = Json(result).into_response();
    if let Some(etag) = etag {
        response.headers_mut().insert(ETAG, etag);
    }
    Ok(response)
}

#[derive(Deserialize)]
pub(crate) struct SearchParams {
    q: String,
    from: Option<String>,
    component: Option<String>,
    #[serde(rename = "type")]
    item_type: Option<String>,
    status: Option<String>,
    milestone: Option<String>,
    limit: Option<usize>,
    budget: Option<usize>,
}

pub(crate) async fn search_items(
    State(state): State<AppState>,
    Q(p): Q<SearchParams>,
) -> Result<Json<SearchResult>, ApiError> {
    let query = SearchQuery {
        text: p.q,
        from: p.from,
        component: p.component,
        kind: p.item_type,
        status: p.status,
        milestone: p.milestone,
        limit: p.limit.unwrap_or(SEARCH_LIMIT),
        budget: p.budget,
    };
    Ok(Json(run(&state, move |repo| search(&repo, &query)).await?))
}

#[derive(Deserialize)]
pub(crate) struct CodeParams {
    target: Option<String>,
    to: Option<String>,
    depth: Option<usize>,
    kind: Option<String>,
    exact: Option<bool>,
    limit: Option<usize>,
    budget: Option<usize>,
}

const CODE_OPS: &str = "find, uses, deps, impact, path, explain, modules, outline, symbol";

pub(crate) async fn code_op(
    State(state): State<AppState>,
    Path(op): Path<String>,
    Q(p): Q<CodeParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !CODE_OPS.split(", ").any(|known| known == op) {
        return Err(ApiError::bad_request(format!("unknown code op `{op}`; use {CODE_OPS}")));
    }
    let value = if op == "modules" {
        run(&state, |repo| Ok(serde_json::to_value(modules(&repo)?))).await?
    } else {
        let Some(target) = p.target.filter(|t| !t.trim().is_empty()) else {
            return Err(ApiError::bad_request(format!("{op} needs `target`")));
        };
        if op == "path" && p.to.is_none() {
            return Err(ApiError::bad_request("path needs `to`"));
        }
        let q = CodeQuery {
            target,
            to: p.to,
            depth: p.depth,
            kind: p.kind,
            exact_only: p.exact.unwrap_or(false),
            limit: p.limit.unwrap_or(0),
            budget: p.budget,
        };
        run(&state, move |repo| {
            Ok(match op.as_str() {
                "find" => serde_json::to_value(code::find(&repo, &q)?),
                "uses" => serde_json::to_value(code::uses(&repo, &q)?),
                "deps" => serde_json::to_value(code::deps(&repo, &q)?),
                "impact" => serde_json::to_value(code_graph::impact(&repo, &q)?),
                "path" => serde_json::to_value(code_graph::path(&repo, &q)?),
                "outline" => serde_json::to_value(code::outline(&repo, &q)?),
                "symbol" => serde_json::to_value(code_symbol::symbol(&repo, &q)?),
                _ => serde_json::to_value(code_graph::explain(&repo, &q)?),
            })
        })
        .await?
    };
    value
        .map(Json)
        .map_err(|err| ApiError::internal(format!("cannot encode the result: {err}")))
}

#[derive(Deserialize)]
pub(crate) struct PartParams {
    /// An architecture id; absent or `none` means the whole repository.
    part: Option<String>,
}

/// The entities of the code and how they relate.
pub(crate) async fn code_domain(
    State(state): State<AppState>,
    Q(p): Q<PartParams>,
) -> Result<Json<DomainModel>, ApiError> {
    Ok(Json(
        run(&state, move |repo| code_domain::domain(&repo, p.part.as_deref())).await?,
    ))
}

#[derive(Deserialize)]
pub(crate) struct WhyParams {
    path: String,
    line: Option<u32>,
    budget: Option<usize>,
}

pub(crate) async fn why(State(state): State<AppState>, Q(p): Q<WhyParams>) -> Result<Json<Why>, ApiError> {
    Ok(Json(
        run(&state, move |repo| engine_why(&repo, &p.path, p.line, p.budget)).await?,
    ))
}

#[derive(Deserialize)]
pub(crate) struct HistoryParams {
    section: Option<String>,
    limit: Option<usize>,
    diff: Option<bool>,
}

pub(crate) async fn history(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Q(p): Q<HistoryParams>,
) -> Result<Json<History>, ApiError> {
    let input = with_section(id, p.section)?;
    let defaults = HistoryOpts::default();
    let opts = HistoryOpts {
        limit: p.limit.unwrap_or(defaults.limit),
        diff: p.diff.unwrap_or(defaults.diff),
    };
    Ok(Json(
        run(&state, move |repo| engine_history(&repo, &input, &opts)).await?,
    ))
}

#[derive(Deserialize)]
pub(crate) struct TimelineParams {
    since: Option<String>,
    until: Option<String>,
    component: Option<String>,
    /// Comma-separated event kinds.
    #[serde(alias = "kind")]
    kinds: Option<String>,
    limit: Option<usize>,
}

pub(crate) async fn timeline(
    State(state): State<AppState>,
    Q(p): Q<TimelineParams>,
) -> Result<Json<Vec<Event>>, ApiError> {
    // A bad value is a bad query (400), not an engine rejection (422).
    let bad = |err: EngineError| ApiError::bad_request(err.to_string());
    let today = Date::today();
    let kinds = p
        .kinds
        .iter()
        .flat_map(|list| list.split(','))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| EventKind::parse(name).map_err(bad))
        .collect::<Result<Vec<_>, _>>()?;
    let opts = TimelineOpts {
        since: p
            .since
            .map(|text| parse_date("since", &text, today))
            .transpose()
            .map_err(bad)?,
        until: p
            .until
            .map(|text| parse_date("until", &text, today))
            .transpose()
            .map_err(bad)?,
        component: p.component,
        kinds,
        limit: p.limit.unwrap_or(0),
    };
    Ok(Json(run(&state, move |repo| engine_timeline(&repo, &opts)).await?))
}

pub(crate) async fn config(State(state): State<AppState>) -> Result<Json<ApiConfig>, ApiError> {
    Ok(Json(run(&state, |repo| Ok(api_config(&repo))).await?))
}

pub(crate) async fn arch(State(state): State<AppState>) -> Result<Json<ArchGraph>, ApiError> {
    Ok(Json(run(&state, |repo| arch_graph(&repo)).await?))
}

/// The docs as a library: what each is, what it says and when it changed.
pub(crate) async fn docs(State(state): State<AppState>) -> Result<Json<Vec<DocRow>>, ApiError> {
    Ok(Json(run(&state, |repo| docs_list::docs(&repo)).await?))
}

/// The high-level design: `stone arch hld`.
pub(crate) async fn arch_hld(State(state): State<AppState>) -> Result<Json<Hld>, ApiError> {
    Ok(Json(run(&state, |repo| hld(&repo)).await?))
}

#[derive(Deserialize)]
pub(crate) struct LldParams {
    /// `false` leaves the service view out; the Service page reads it from `explain`.
    service: Option<bool>,
}

/// The low-level design of the container the path names: `stone arch lld <container>`.
pub(crate) async fn arch_lld(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Q(p): Q<LldParams>,
) -> Result<Json<Lld>, ApiError> {
    let with_service = p.service.unwrap_or(true);
    Ok(Json(
        run(&state, move |repo| {
            if with_service {
                lld(&repo, &id)
            } else {
                lld_docs(&repo, &id)
            }
        })
        .await?,
    ))
}

/// The C4 context diagram as Mermaid text: `stone arch c4 context`.
pub(crate) async fn arch_c4_context(State(state): State<AppState>) -> Result<Json<C4Source>, ApiError> {
    Ok(Json(
        run(&state, |repo| c4_context(&repo).map(|source| C4Source { source })).await?,
    ))
}

#[derive(Deserialize)]
pub(crate) struct C4Params {
    /// The system to draw; optional while the store has one product.
    system: Option<String>,
}

/// The C4 container diagram of a system: `stone arch c4 container [<system>]`.
pub(crate) async fn arch_c4_container(
    State(state): State<AppState>,
    Q(p): Q<C4Params>,
) -> Result<Json<C4Source>, ApiError> {
    Ok(Json(
        run(&state, move |repo| {
            c4_container(&repo, p.system.as_deref()).map(|source| C4Source { source })
        })
        .await?,
    ))
}

/// The C4 component diagram of the container the path names: `stone arch c4 component <container>`.
pub(crate) async fn arch_c4_component(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<C4Source>, ApiError> {
    Ok(Json(
        run(&state, move |repo| {
            c4_component(&repo, &id).map(|source| C4Source { source })
        })
        .await?,
    ))
}

/// Anything that matched no route: the web UI for a page GET, a JSON 404 for the rest.
pub(crate) async fn fallback(method: Method, uri: Uri) -> Response {
    let path = uri.path();
    let is_api = path == "/api" || path.starts_with("/api/");
    if !is_api && (method == Method::GET || method == Method::HEAD) {
        return ui::page(path);
    }
    ApiError::not_found("no such route; the API lives under /api").into_response()
}
