//! The product and work routes: what the product is, what each feature connects to and what was
//! done when. Handlers parse the request and call one engine function, like the rest of the API.

use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use carvestone_engine::EngineError;
use carvestone_engine::activity::{ActivityGroup, ActivityOpts, Period, activity as engine_activity};
use carvestone_engine::product::{
    FeatureDetail, Product, feature as engine_feature, product as engine_product, task_features,
};
use carvestone_engine::timeline::{Date, parse_date};
use carvestone_engine::work_links::task_commits as engine_task_commits;
use serde::Deserialize;

use crate::AppState;
use crate::error::ApiError;
use crate::extract::{Q, run};

/// The tails of the routes under `/api/tasks/`; axum takes no text after a `{*wildcard}`.
const COMMITS_TAIL: &str = "/commits";
const FEATURES_TAIL: &str = "/features";

pub(crate) async fn product(State(state): State<AppState>) -> Result<Json<Product>, ApiError> {
    Ok(Json(run(&state, |repo| engine_product(&repo)).await?))
}

pub(crate) async fn feature(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<FeatureDetail>, ApiError> {
    Ok(Json(run(&state, move |repo| engine_feature(&repo, &id)).await?))
}

/// `GET /api/tasks/{id}/commits` and `GET /api/tasks/{id}/features`.
pub(crate) async fn task_answer(State(state): State<AppState>, Path(path): Path<String>) -> Result<Response, ApiError> {
    if let Some(id) = path.strip_suffix(COMMITS_TAIL).map(str::to_string) {
        let commits = run(&state, move |repo| engine_task_commits(&repo, &id)).await?;
        return Ok(Json(commits).into_response());
    }
    if let Some(id) = path.strip_suffix(FEATURES_TAIL).map(str::to_string) {
        let features = run(&state, move |repo| task_features(&repo, &id)).await?;
        return Ok(Json(features).into_response());
    }
    Err(ApiError::not_found(
        "no such route; ask /api/tasks/<id>/commits or /api/tasks/<id>/features",
    ))
}

#[derive(Deserialize)]
pub(crate) struct ActivityParams {
    period: Option<String>,
    from: Option<String>,
    to: Option<String>,
}

pub(crate) async fn activity(
    State(state): State<AppState>,
    Q(p): Q<ActivityParams>,
) -> Result<Json<Vec<ActivityGroup>>, ApiError> {
    // A bad value is a bad query (400), not an engine rejection (422).
    let bad = |err: EngineError| ApiError::bad_request(err.to_string());
    let today = Date::today();
    let day =
        |flag: &str, text: Option<String>| text.map(|text| parse_date(flag, &text, today)).transpose().map_err(bad);
    let opts = ActivityOpts {
        period: Period::parse(p.period.as_deref().unwrap_or("week")).map_err(bad)?,
        from: day("from", p.from)?,
        to: day("to", p.to)?,
    };
    Ok(Json(run(&state, move |repo| engine_activity(&repo, &opts)).await?))
}
