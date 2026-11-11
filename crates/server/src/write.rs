//! Write routes: the same engine calls `stone edit` and `stone new` make, so the bytes on disk
//! are the ones the CLI would write.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header::IF_MATCH;
use axum::http::{HeaderMap, StatusCode};
use carvestone_engine::api::{CreateReply, CreateRequest, FieldValue, Pairs, PatchReply, PatchRequest};
use carvestone_engine::create::NewItem;
use carvestone_engine::edit::Edit;
use carvestone_engine::fields::SetValue;
use carvestone_engine::query::etag_of;

use crate::AppState;
use crate::error::ApiError;
use crate::extract::{Body, run};

fn values(pairs: Pairs<FieldValue>) -> Vec<(String, SetValue)> {
    pairs
        .0
        .into_iter()
        .map(|(key, value)| (key, value.into_value()))
        .collect()
}

/// The etag a request asks to match: the `If-Match` header, the `if_match` body field, or `None`.
/// Both given must agree. Quotes and a weak marker are dropped, `*` means any version, and a
/// list of etags is refused because the write can only compare one.
fn wanted_etag(headers: &HeaderMap, field: Option<String>) -> Result<Option<String>, ApiError> {
    let header = match headers.get(IF_MATCH) {
        None => None,
        Some(value) => {
            let text = value
                .to_str()
                .map_err(|_| ApiError::bad_request("If-Match must be one etag"))?;
            if text.contains(',') {
                return Err(ApiError::bad_request("If-Match must be one etag"));
            }
            Some(text)
        }
    };
    let bare = |tag: &str| {
        let tag = tag.trim();
        tag.strip_prefix("W/").unwrap_or(tag).trim_matches('"').to_string()
    };
    let (header, field) = (header.map(bare), field.as_deref().map(bare));
    match (header, field) {
        (Some(h), Some(f)) if h != f => Err(ApiError::bad_request(
            "the If-Match header and the if_match field differ; send one",
        )),
        (Some(tag), _) | (None, Some(tag)) => Ok((tag != "*").then_some(tag)),
        (None, None) => Ok(None),
    }
}

pub(crate) async fn patch_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Body(body): Body<PatchRequest>,
) -> Result<Json<PatchReply>, ApiError> {
    let req = Edit {
        set: values(body.set),
        unset: body.unset,
        section: body.section.map(|s| (s.name, s.text)),
        check_ac: body.check_ac,
        note: body.note,
    };
    let if_match = wanted_etag(&headers, body.if_match)?;
    let dry_run = body.dry_run;
    let bus = state.bus.clone();
    let reply = run(&state, move |repo| {
        let written = repo.edit_if_match(&id, &req, dry_run, if_match.as_deref())?;
        let etag = if dry_run {
            repo.etag(&id)?
        } else {
            etag_of(&written.text)
        };
        if written.changed && !dry_run {
            bus.publish_write(&repo, &written.path, &written.text);
        }
        Ok(PatchReply {
            id: written.id,
            changed: written.changed,
            etag,
            text: dry_run.then_some(written.text),
        })
    })
    .await?;
    Ok(Json(reply))
}

pub(crate) async fn create_item(
    State(state): State<AppState>,
    Body(body): Body<CreateRequest>,
) -> Result<(StatusCode, Json<CreateReply>), ApiError> {
    let req = NewItem {
        type_name: body.item_type,
        title: body.title,
        slug: body.slug,
        set: values(body.fields),
        sections: body.sections.0,
        body: None,
        links: Vec::new(),
        left_out: Vec::new(),
    };
    let bus = state.bus.clone();
    let reply = run(&state, move |repo| {
        let written = repo.create(&req, false)?;
        bus.publish_write(&repo, &written.path, &written.text);
        Ok(CreateReply {
            etag: etag_of(&written.text),
            id: written.id,
            path: written.path,
        })
    })
    .await?;
    Ok((StatusCode::CREATED, Json(reply)))
}
