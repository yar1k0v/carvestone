//! What extraction returns: declarations, reference sites, imports and parse
//! warnings for one file. Plain data, no parser types, so resolvers and the
//! index only see facts.

use std::path::Path;

/// The languages the graph reads. Chosen by file extension, never by content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Lang {
    CSharp,
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Dart,
}

impl Lang {
    /// The name stored in `files.lang` and shown in queries.
    pub fn as_str(self) -> &'static str {
        match self {
            Lang::CSharp => "csharp",
            Lang::Rust => "rust",
            Lang::TypeScript => "typescript",
            Lang::JavaScript => "javascript",
            Lang::Python => "python",
            Lang::Dart => "dart",
        }
    }
}

/// The language of `path`, or `None` for a file the graph does not read. Generated Dart
/// (`*.g.dart` from json_serializable and friends, `*.freezed.dart`) is not read: it is committed
/// but never written by hand, and it would flood `uses` with its machine-made patterns.
pub fn lang_of(path: &str) -> Option<Lang> {
    let ext = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "dart" => {
            let lower = path.to_ascii_lowercase();
            if lower.ends_with(".g.dart") || lower.ends_with(".freezed.dart") {
                None
            } else {
                Some(Lang::Dart)
            }
        }
        "cs" => Some(Lang::CSharp),
        "rs" => Some(Lang::Rust),
        "ts" | "tsx" => Some(Lang::TypeScript),
        "js" | "jsx" | "mjs" | "cjs" => Some(Lang::JavaScript),
        "py" => Some(Lang::Python),
        _ => None,
    }
}

/// Whether a repo path is test code: a test directory, or a test file by the naming habits of the
/// supported languages (`OrderTests.cs`, `store_test.rs`, `widget_test.dart`, `a.test.ts`).
/// Name guesses rank it last.
pub fn is_test_path(path: &str) -> bool {
    let mut segments: Vec<&str> = path.split('/').collect();
    let file = segments.pop().unwrap_or("");
    let in_test_dir = segments.iter().any(|raw| {
        let dir = raw.to_ascii_lowercase();
        matches!(
            dir.as_str(),
            "test" | "tests" | "spec" | "specs" | "__tests__" | "__mocks__" | "integration_test" | "test_driver"
        ) || dir.ends_with("tests")
            || dir.ends_with(".test")
            || dir.ends_with(".spec")
    });
    if in_test_dir {
        return true;
    }
    let lower = file.to_ascii_lowercase();
    let stem = file.split('.').next().unwrap_or(file);
    lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.starts_with("test_")
        || lower == "conftest.py"
        || stem.eq_ignore_ascii_case("test")
        || stem.to_ascii_lowercase().ends_with("_test")
        || stem.ends_with("Test")
        || stem.ends_with("Tests")
}

/// A declaration kind. `parent` in [`Decl`] points at the enclosing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclKind {
    Project,
    Module,
    File,
    Type,
    Function,
    Method,
    Member,
}

impl DeclKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DeclKind::Project => "project",
            DeclKind::Module => "module",
            DeclKind::File => "file",
            DeclKind::Type => "type",
            DeclKind::Function => "function",
            DeclKind::Method => "method",
            DeclKind::Member => "member",
        }
    }
}

/// How a reference site names its target. `Dispatch` is only ever an edge
/// (an interface or abstract member to an implementer's), never a site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefKind {
    Imports,
    Calls,
    Inherits,
    Implements,
    References,
    Dispatch,
}

impl RefKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RefKind::Imports => "imports",
            RefKind::Calls => "calls",
            RefKind::Inherits => "inherits",
            RefKind::Implements => "implements",
            RefKind::References => "references",
            RefKind::Dispatch => "dispatch",
        }
    }
}

/// A declaration in a file. Lines are 1-based and inclusive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decl {
    pub name: String,
    /// Fully qualified in the language's own terms: `Shop.Orders.OrderService.Place`.
    pub qname: String,
    pub kind: DeclKind,
    /// Index into the same file's `decls`.
    pub parent: Option<usize>,
    /// The declaration header without its body.
    pub signature: String,
    /// The leading comment block, markers removed.
    pub doc: Option<String>,
    pub line_start: u32,
    pub line_end: u32,
}

/// A name used at a site: a call, a base type, or a plain reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefSite {
    pub name: String,
    /// What precedes the name in the source, when the site has one.
    pub qualifier: Option<String>,
    pub kind: RefKind,
    pub line: u32,
    /// Index into the file's `decls` of the declaration the site sits in.
    pub scope: Option<usize>,
}

/// One `using`, `import` or `use` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDecl {
    /// The imported namespace, module or path, as written.
    pub path: String,
    /// The local name for an alias import.
    pub alias: Option<String>,
    /// `using X.*;`, `from x import *`.
    pub glob: bool,
    /// `using static X;`.
    pub is_static: bool,
    /// `global using X;`: applies to every file of the project.
    pub global: bool,
    pub line: u32,
}

/// A problem found while parsing. Extraction keeps going after one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub message: String,
    pub line: u32,
}

/// Everything extraction learned from one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFacts {
    pub path: String,
    pub lang: Lang,
    pub decls: Vec<Decl>,
    pub refs: Vec<RefSite>,
    pub imports: Vec<ImportDecl>,
    pub warnings: Vec<Warning>,
}

impl FileFacts {
    pub fn new(lang: Lang, path: &str) -> Self {
        Self {
            path: path.to_string(),
            lang,
            decls: Vec::new(),
            refs: Vec::new(),
            imports: Vec::new(),
            warnings: Vec::new(),
        }
    }
}
