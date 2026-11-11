//! The four read tools: argument structs, schemas and handlers. Each handler
//! parses its arguments, calls one engine function and renders the result as
//! compact JSON; no tool logic lives here.

use std::sync::Arc;

use carvestone_engine::EngineError;
use carvestone_engine::Repo;
use carvestone_engine::code::{self, CodeQuery};
use carvestone_engine::code_graph;
use carvestone_engine::query::ListFilter;
use carvestone_engine::search::{SearchQuery, search};
use rmcp::model::{CallToolResult, JsonObject, Tool};
use serde::{Deserialize, Serialize};

use crate::errors;

/// Hits a search returns when `limit` is absent; the CLI default.
const SEARCH_LIMIT: usize = 10;
/// Tokens a search may spend when `budget` is absent: agents read what they get.
const SEARCH_BUDGET: usize = 400;
/// Tasks `list` returns for `ready: true`.
const READY_LIMIT: usize = 5;

/// The tools in the order `tools/list` reports them.
pub fn definitions() -> Vec<Tool> {
    vec![
        tool::<SearchArgs>(
            "search",
            "Search sections by words; hits carry id, anchor, path, lines and a snippet.",
        ),
        tool::<GetArgs>(
            "get",
            "Read an item, or one section with `section`, with its backlinks.",
        ),
        tool::<ListArgs>(
            "list",
            "List items by type, status, milestone or component; `ready` gives the tasks to start next.",
        ),
        tool::<CodeArgs>(
            "code",
            "Query the code graph: find symbols, uses (who depends on a target), deps, impact, path between two targets, explain a component.",
        ),
    ]
}

/// Runs the named tool.
pub fn call(name: &str, args: &JsonObject, repo: &Repo) -> CallToolResult {
    match name {
        "search" => run(name, args, |a: SearchArgs| search_tool(repo, a)),
        "get" => run(name, args, |a: GetArgs| get_tool(repo, a)),
        "list" => run(name, args, |a: ListArgs| list_tool(repo, a)),
        "code" => run(name, args, |a: CodeArgs| code_tool(repo, a)),
        other => errors::tool_error(format!("unknown tool `{other}`; call `tools/list` for the names")),
    }
}

fn tool<T: schemars::JsonSchema>(name: &'static str, description: &'static str) -> Tool {
    Tool::new(name, description, input_schema::<T>())
}

/// A tool's input schema, built from its argument struct.
fn input_schema<T: schemars::JsonSchema>() -> Arc<JsonObject> {
    match serde_json::to_value(schemars::schema_for!(T)) {
        Ok(serde_json::Value::Object(schema)) => Arc::new(schema),
        _ => Arc::new(JsonObject::new()),
    }
}

/// Parses the arguments, runs `body` and renders its value; any failure is a tool error.
fn run<A, T>(tool: &str, args: &JsonObject, body: impl FnOnce(A) -> Result<T, EngineError>) -> CallToolResult
where
    A: serde::de::DeserializeOwned,
    T: Serialize,
{
    let args: A = match serde_json::from_value(serde_json::Value::Object(args.clone())) {
        Ok(args) => args,
        Err(err) => return errors::tool_error(format!("invalid arguments for {tool}: {err}")),
    };
    match body(args) {
        Ok(value) => match serde_json::to_string(&value) {
            Ok(text) => errors::text(text),
            Err(_) => errors::tool_error(errors::INTERNAL),
        },
        Err(err) => errors::tool_error(err.to_string()),
    }
}

/// Arguments of `search`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchArgs {
    /// Words to find; several words must all match.
    query: String,
    /// Rank sections of items linked to this item first.
    from: Option<String>,
    /// Only items that list this component or live under it.
    component: Option<String>,
    /// Only items of this type.
    r#type: Option<String>,
    status: Option<String>,
    milestone: Option<String>,
    /// Most hits to return (default 10).
    limit: Option<usize>,
    /// Stop adding hits once their lines cost this many tokens (default 400).
    budget: Option<usize>,
}

fn search_tool(repo: &Repo, args: SearchArgs) -> Result<carvestone_engine::search::SearchResult, EngineError> {
    search(
        repo,
        &SearchQuery {
            text: args.query,
            from: args.from,
            component: args.component,
            kind: args.r#type,
            status: args.status,
            milestone: args.milestone,
            limit: args.limit.unwrap_or(SEARCH_LIMIT),
            budget: Some(args.budget.unwrap_or(SEARCH_BUDGET)),
        },
    )
}

/// Arguments of `get`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetArgs {
    /// Item id such as `CS-12`, `ADR-3` or `docs/prd`.
    id: String,
    /// A section anchor; only that section is returned.
    section: Option<String>,
}

fn get_tool(repo: &Repo, args: GetArgs) -> Result<carvestone_engine::query::GetResult, EngineError> {
    match args.section {
        Some(_) if args.id.contains('#') => Err(EngineError::Invalid(
            "pass either `id#anchor` or `section`, not both".into(),
        )),
        Some(section) => repo.get(&format!("{}#{section}", args.id)),
        None => repo.get(&args.id),
    }
}

/// Arguments of `list`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListArgs {
    r#type: Option<String>,
    status: Option<String>,
    milestone: Option<String>,
    component: Option<String>,
    /// With type `task`: the todo tasks whose dependencies are done, best first (5 at most).
    ready: Option<bool>,
}

fn list_tool(repo: &Repo, args: ListArgs) -> Result<Vec<carvestone_engine::query::ListRow>, EngineError> {
    if args.ready != Some(true) {
        return repo.list(&ListFilter {
            item_type: args.r#type,
            status: args.status,
            milestone: args.milestone,
            component: args.component,
            ..ListFilter::default()
        });
    }
    if args.r#type.as_deref().is_some_and(|t| t != "task") {
        return Err(EngineError::Invalid(
            "`ready` applies to tasks; set type to task or leave it out".into(),
        ));
    }
    if args.status.is_some() || args.milestone.is_some() || args.component.is_some() {
        return Err(EngineError::Invalid(
            "`ready` cannot be combined with status, milestone or component".into(),
        ));
    }
    repo.next(READY_LIMIT)
}

/// Arguments of `code`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CodeArgs {
    /// find, uses, deps, impact, path or explain.
    op: String,
    /// A symbol (qualified or plain name), a file path or `path:line`; for `explain` a component id; for `path` the start.
    target: String,
    /// `path` only: the other end.
    to: Option<String>,
    /// `impact` only: how many hops to follow (default 3).
    depth: Option<usize>,
    /// Leave out inferred edges.
    #[serde(default)]
    exact: bool,
    /// Stop adding hits once their lines cost this many tokens (chars / 4).
    budget: Option<usize>,
}

fn code_tool(repo: &Repo, args: CodeArgs) -> Result<serde_json::Value, EngineError> {
    let q = CodeQuery {
        target: args.target,
        to: args.to,
        depth: args.depth,
        exact_only: args.exact,
        budget: args.budget,
        ..CodeQuery::default()
    };
    let value = match args.op.as_str() {
        "find" => serde_json::to_value(code::find(repo, &q)?),
        "uses" => serde_json::to_value(code::uses(repo, &q)?),
        "deps" => serde_json::to_value(code::deps(repo, &q)?),
        "impact" => serde_json::to_value(code_graph::impact(repo, &q)?),
        "path" if q.to.is_none() => return Err(EngineError::Invalid("path needs `to`".into())),
        "path" => serde_json::to_value(code_graph::path(repo, &q)?),
        "explain" => serde_json::to_value(code_graph::explain(repo, &q)?),
        other => {
            return Err(EngineError::Invalid(format!(
                "unknown op `{other}`; use find, uses, deps, impact, path or explain"
            )));
        }
    };
    value.map_err(|_| EngineError::Invalid(errors::INTERNAL.to_string()))
}
