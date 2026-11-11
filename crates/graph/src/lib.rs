//! The code graph without a database: tree-sitter extraction into plain fact
//! types, project-file facts and (from the resolver batch on) the per-language
//! resolvers. The engine stores the facts in the index; nothing here touches
//! SQLite or the filesystem.

mod csharp;
mod csharp_types;
mod dart;
mod dispatch;
mod facts;
mod louvain;
mod project;
mod python;
mod resolve;
mod resolve_csharp;
mod resolve_dart;
mod resolve_python;
mod resolve_rust;
mod resolve_ts;
mod rust;
mod rust_modules;
mod scip_read;
mod ts;

/// Bump this whenever extraction or resolution output changes (a new fact, a
/// different qname, an edge rule). A stored index with another value drops its
/// code rows on the next refresh and fills them again.
pub const CODE_GRAPH_VERSION: u32 = 17;

pub use dispatch::dispatch;
pub use facts::{Decl, DeclKind, FileFacts, ImportDecl, Lang, RefKind, RefSite, Warning, is_test_path, lang_of};
pub use louvain::louvain;
pub use project::{PathAlias, ProjectFacts, ProjectKind, ProjectReference, project_facts};
pub use resolve::{Bucket, Edge, Node, Resolver, SymRef, WORKSPACE_SCOPE, World, finish, resolve, resolver_for};
pub use scip_read::{
    Encoding, ScipDocument, ScipIndex, ScipOccurrence, ScipSymbol, parse_scip, scip_symbol, slice_line,
};

/// Parse one file into facts. Pure: the same bytes always give equal facts.
/// A language whose extractor has not landed yet yields no declarations.
pub fn extract(lang: Lang, path: &str, src: &str) -> FileFacts {
    match lang {
        Lang::CSharp => csharp::extract_csharp(path, src),
        Lang::Rust => rust::extract_rust(path, src),
        Lang::TypeScript => ts::extract_ts(path, src),
        Lang::JavaScript => ts::extract_js(path, src),
        Lang::Python => python::extract_python(path, src),
        Lang::Dart => dart::extract_dart(path, src),
    }
}
