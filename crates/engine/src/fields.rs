//! Field changes shared by `new`, `edit`, `link` and `task move`: which keys may be set, what a value may be and how a list changes.

use carvestone_core::{IdScheme, ItemType, LIST_KEYS, SCALAR_KEYS, section_text, sections};
use carvestone_store::{Config, Document};
use serde_norway::Value;

use crate::check_links::RELATION_KEYS;
use crate::create::today;
use crate::repo::EngineError;
use crate::resolve::Snapshot;

/// What `apply_set` needs to judge a value.
pub(crate) struct SetCtx<'a> {
    pub snapshot: &'a Snapshot,
    pub config: &'a Config,
    pub ty: ItemType,
}

/// `type` is fixed at creation and provenance is written by stone itself: a hand-set `verified` would forge trust.
pub(crate) fn settable(key: &str) -> Result<(), EngineError> {
    if SCALAR_KEYS.contains(&key) || LIST_KEYS.contains(&key) {
        return Ok(());
    }
    let why = match key {
        "type" => "the type is fixed when the item is created".to_string(),
        "generated" | "verified" => "stone writes provenance itself; it is never set by hand".to_string(),
        _ => format!(
            "it is not a field stone knows; use one of: {}",
            SCALAR_KEYS
                .iter()
                .chain(LIST_KEYS.iter())
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    Err(EngineError::Invalid(format!("cannot change `{key}`: {why}")))
}

/// One field change as `stone edit` and `stone new` take it: the CLI's text syntax, or a list that
/// replaces a list field with exactly the entries given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetValue {
    Text(String),
    List(Vec<String>),
}

impl From<String> for SetValue {
    fn from(text: String) -> Self {
        SetValue::Text(text)
    }
}

impl From<&str> for SetValue {
    fn from(text: &str) -> Self {
        SetValue::Text(text.to_string())
    }
}

/// Splits a list request on commas, dropping blanks.
fn items(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// True when two relation texts name the same target, ignoring case and zero padding.
fn same_ref(scheme: &IdScheme, a: &str, b: &str) -> bool {
    let canon = |text: &str| {
        let canonical = match scheme.parse(text.trim()) {
            Ok(r) => scheme.link_text(&r),
            Err(_) => text.trim().to_string(),
        };
        canonical.to_lowercase()
    };
    canon(a) == canon(b)
}

fn canon_item(ctx: &SetCtx, relation: bool, text: String) -> Result<String, EngineError> {
    if relation {
        canonical_ref(ctx.snapshot, &text)
    } else {
        Ok(text)
    }
}

/// The entries of a list as the file should hold them: canonicalised, blanks skipped, duplicates dropped.
fn canon_list(ctx: &SetCtx, key: &str, texts: impl IntoIterator<Item = String>) -> Result<Vec<String>, EngineError> {
    let relation = RELATION_KEYS.contains(&key);
    let scheme = &ctx.snapshot.scheme;
    let mut out: Vec<String> = Vec::new();
    for text in texts {
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let text = canon_item(ctx, relation, text.to_string())?;
        if !out.iter().any(|have| same_ref(scheme, have, &text)) {
            out.push(text);
        }
    }
    Ok(out)
}

/// Saves `next` only when the file holds something else; a list compared as text is never rewritten twice.
fn save_list(doc: &mut Document, key: &str, next: Vec<String>) -> Result<bool, EngineError> {
    if next == doc.get_list(key).unwrap_or_default() {
        return Ok(false);
    }
    doc.set(key, Value::Sequence(next.into_iter().map(Value::String).collect()))?;
    Ok(true)
}

/// The CLI form: `a,b` replaces, `+a` adds, `-a` removes.
fn set_list(ctx: &SetCtx, doc: &mut Document, key: &str, raw: &str) -> Result<bool, EngineError> {
    let current = doc.get_list(key).unwrap_or_default();
    let scheme = &ctx.snapshot.scheme;
    let next: Vec<String> = if let Some(add) = raw.strip_prefix('+') {
        let mut out = current.clone();
        for text in canon_list(ctx, key, items(add))? {
            if !out.iter().any(|have| same_ref(scheme, have, &text)) {
                out.push(text);
            }
        }
        out
    } else if let Some(gone) = raw.strip_prefix('-') {
        let drop = items(gone);
        current
            .iter()
            .filter(|have| !drop.iter().any(|d| same_ref(scheme, have, d)))
            .cloned()
            .collect()
    } else {
        canon_list(ctx, key, items(raw))?
    };
    save_list(doc, key, next)
}

/// A list sent as a list: it replaces the field with exactly these entries, never read as `+`/`-` syntax.
fn apply_list(ctx: &SetCtx, doc: &mut Document, key: &str, values: Vec<String>) -> Result<bool, EngineError> {
    settable(key)?;
    if !LIST_KEYS.contains(&key) {
        return Err(EngineError::Invalid(format!(
            "`{key}` is not a list field; send a single value"
        )));
    }
    let next = canon_list(ctx, key, values)?;
    save_list(doc, key, next)
}

fn set_scalar(ctx: &SetCtx, doc: &mut Document, key: &str, raw: &str) -> Result<bool, EngineError> {
    let value = raw.trim();
    if key == "title" && value.is_empty() {
        return Err(EngineError::Invalid("the title cannot be empty".into()));
    }
    let value = if RELATION_KEYS.contains(&key) {
        if value.is_empty() {
            return Err(EngineError::Invalid(format!(
                "`{key}` cannot be empty; remove it with --unset {key}"
            )));
        }
        canonical_ref(ctx.snapshot, value)?
    } else {
        check_vocab(ctx.config, ctx.ty, key, value)?;
        value.to_string()
    };
    // The raw text, not the parsed value: `order: 2` set to `2` must not become `'2'`.
    if doc.get_str(key).as_deref() == Some(value.as_str()) {
        return Ok(false);
    }
    doc.set(key, Value::String(value))?;
    Ok(true)
}

/// Applies one `--set key=value` to the document. `Ok(true)` when the document changed.
pub(crate) fn apply_set(ctx: &SetCtx, doc: &mut Document, key: &str, raw: &str) -> Result<bool, EngineError> {
    settable(key)?;
    if LIST_KEYS.contains(&key) {
        set_list(ctx, doc, key, raw)
    } else {
        set_scalar(ctx, doc, key, raw)
    }
}

/// Keeps `closed` in step with a task whose status just changed from `was`: finishing it records
/// today unless a day was given, reopening it drops the day. Archiving a finished task is not
/// reopening it, and a task that stays done keeps what it has, so repeating the move writes nothing.
pub(crate) fn sync_closed(ctx: &SetCtx, doc: &mut Document, was: Option<&str>) -> Result<(), EngineError> {
    let status = doc.get_str("status");
    let now = status.as_deref();
    let finished = |status: Option<&str>| status.is_some_and(|s| ctx.config.task_finished(s));
    if now == Some("done") && was != Some("done") && doc.get_str("closed").is_none() {
        apply_set(ctx, doc, "closed", &today())?;
    } else if finished(was) && !finished(now) {
        doc.remove("closed")?;
    }
    Ok(())
}

/// Applies one `set` pair, whichever form it came in: CLI text or a list sent as a list.
pub(crate) fn apply_value(ctx: &SetCtx, doc: &mut Document, key: &str, value: &SetValue) -> Result<bool, EngineError> {
    match value {
        SetValue::Text(raw) => apply_set(ctx, doc, key, raw),
        SetValue::List(values) => apply_list(ctx, doc, key, values.clone()),
    }
}

/// Appends link targets to `links` for a new item, each as written and in the order given; a
/// target that names nothing yet is kept (`stone check` reports it later), and one already present
/// is not added twice.
pub(crate) fn add_links(scheme: &IdScheme, doc: &mut Document, targets: &[String]) -> Result<bool, EngineError> {
    let mut next = doc.get_list("links").unwrap_or_default();
    for target in targets.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        if !next.iter().any(|have| same_ref(scheme, have, target)) {
            next.push(target.to_string());
        }
    }
    save_list(doc, "links", next)
}

/// Applies one `--unset key`. `Ok(true)` when the key was there.
pub(crate) fn apply_unset(doc: &mut Document, key: &str) -> Result<bool, EngineError> {
    settable(key)?;
    Ok(doc.remove(key)?)
}

/// `status`, `kind` and `priority` against the config vocabulary, worded like `stone check`.
pub(crate) fn check_vocab(config: &Config, ty: ItemType, key: &str, value: &str) -> Result<(), EngineError> {
    let allowed = match key {
        "status" => config.statuses_for(ty),
        "kind" if ty == ItemType::Task => config.task_kinds.clone(),
        "priority" if ty == ItemType::Task => config.priorities.clone(),
        _ => return Ok(()),
    };
    if allowed.iter().any(|a| a == value) {
        return Ok(());
    }
    Err(EngineError::Invalid(format!(
        "{key} `{value}` is not valid for a {}; use one of: {}",
        ty.as_str(),
        allowed.join(", ")
    )))
}

/// The canonical text of a relation value: the item's label, plus the found section's anchor when one was given.
pub(crate) fn canonical_ref(snapshot: &Snapshot, value: &str) -> Result<String, EngineError> {
    let (entry, anchor) = snapshot.resolve(value.trim())?;
    let label = snapshot.label(entry);
    match anchor {
        Some(anchor) => {
            let section = snapshot.find_section(entry, &anchor)?;
            Ok(format!("{label}#{}", section.anchor))
        }
        None => Ok(label),
    }
}

/// True when the body has an `Outcome` section with text under its heading, as `stone check` requires of a done task.
pub(crate) fn has_outcome(body: &str, first_line: usize) -> bool {
    has_filled_section(body, first_line, "Outcome")
}

/// Refuses a task body that may not be done: no `## Outcome` always, no `## Plan` when the store sets
/// `done_needs_plan`. The message names every missing section and how to add it.
pub(crate) fn check_done_sections(config: &Config, id: &str, body: &str, first_line: usize) -> Result<(), EngineError> {
    let no_outcome = !has_outcome(body, first_line);
    let no_plan = config.done_needs_plan && !has_filled_section(body, first_line, "Plan");
    match (no_plan, no_outcome) {
        (false, false) => Ok(()),
        (false, true) if !config.done_needs_plan => Err(EngineError::NoOutcome { id: id.to_string() }),
        (plan, outcome) => {
            let missing: Vec<(&str, &str)> = [(plan, "plan", "## Plan"), (outcome, "outcome", "## Outcome")]
                .into_iter()
                .filter_map(|(absent, name, heading)| absent.then_some((name, heading)))
                .collect();
            let names: Vec<&str> = missing.iter().map(|(_, heading)| *heading).collect();
            let adds: Vec<String> = missing
                .iter()
                .map(|(name, _)| format!("`stone edit {id} --section {name} --text \"...\"`"))
                .collect();
            Err(EngineError::Invalid(format!(
                "{id} cannot be done: it has no {} (`done_needs_plan` is on in stone/config.yml); add with {}",
                names.join(" and no "),
                adds.join(" and ")
            )))
        }
    }
}

/// True when the body has a section titled `title` with text under its heading.
fn has_filled_section(body: &str, first_line: usize, title: &str) -> bool {
    sections(body, first_line)
        .iter()
        .find(|s| s.title.eq_ignore_ascii_case(title))
        .is_some_and(|s| {
            section_text(body, first_line, s)
                .lines()
                .skip(1)
                .any(|l| !l.trim().is_empty())
        })
}
