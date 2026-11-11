//! carvestone-engine: the operations every interface shares.
//!
//! A `Repo` owns the store and its config. CLI, MCP and HTTP only parse
//! input and render output; all behaviour lives here so the adapters cannot
//! drift.

pub mod activity;
pub mod agent_block;
pub mod api;
pub mod api_config;
pub mod arch_c4;
pub mod arch_gaps;
pub mod arch_graph;
pub mod brief;
pub mod changed_files;
pub mod changes;
pub mod check;
mod check_fields;
mod check_files;
mod check_links;
pub mod code;
pub mod code_domain;
pub mod code_entries;
mod code_gap;
pub mod code_graph;
pub mod code_index;
mod code_map;
pub mod code_native;
mod code_rank;
pub mod code_short;
pub mod code_symbol;
pub mod commit_msg;
pub mod create;
pub mod design;
mod doc_file;
pub mod docs_list;
pub mod edit;
mod edit_config;
mod edit_tasks;
pub mod fields;
pub mod fresh;
pub mod git_cache;
pub mod guide;
pub mod history;
pub mod links;
pub mod modules;
mod new_item;
pub mod owner;
pub mod precedents;
pub mod product;
pub mod project_map;
pub mod query;
pub mod ready;
pub mod refresh;
pub mod remove;
pub mod rename;
pub mod renumber;
pub mod repo;
pub mod repos;
pub mod resolve;
pub mod scan;
pub mod search;
pub mod service_map;
pub mod session_hook;
pub mod stale;
pub mod tiers;
pub mod timeline;
pub mod units;
pub mod why;
pub mod work_links;

pub use check::{Problem, Severity};
pub use check_links::{CodeLink, resolve_code_link};
pub use repo::{EngineError, ErrorKind, Repo};
pub use repos::{RepoRef, Repos};
pub use resolve::Snapshot;
