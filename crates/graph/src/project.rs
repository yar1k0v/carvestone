//! Project-file facts: what one `*.csproj`, `Cargo.toml`, `package.json`,
//! `tsconfig.json`, `pyproject.toml` or `pubspec.yaml` says about itself, what it references
//! and which directories hold its code.
//!
//! Every parser here is deliberately tolerant: project files are hand-edited
//! and truncated input must never panic, so a scanner stops at the first thing
//! it does not understand and keeps what it already has.

use serde_json::Value;

/// The project files the graph understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectKind {
    Csproj,
    Cargo,
    Npm,
    TsConfig,
    PyProject,
    /// A Dart or Flutter package: `pubspec.yaml`.
    Pubspec,
}

impl ProjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectKind::Csproj => "csproj",
            ProjectKind::Cargo => "cargo",
            ProjectKind::Npm => "npm",
            ProjectKind::TsConfig => "tsconfig",
            ProjectKind::PyProject => "pyproject",
            ProjectKind::Pubspec => "pubspec",
        }
    }

    /// The kind of a project file, by its file name.
    pub fn of(path: &str) -> Option<ProjectKind> {
        let name = file_name(path).to_ascii_lowercase();
        if name.ends_with(".csproj") {
            Some(ProjectKind::Csproj)
        } else {
            match name.as_str() {
                "cargo.toml" => Some(ProjectKind::Cargo),
                "package.json" => Some(ProjectKind::Npm),
                "tsconfig.json" => Some(ProjectKind::TsConfig),
                "pyproject.toml" => Some(ProjectKind::PyProject),
                "pubspec.yaml" => Some(ProjectKind::Pubspec),
                _ => None,
            }
        }
    }
}

/// A path to another project, as it appears after normalizing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectReference {
    /// The project name when the file states one, otherwise the file stem.
    pub name: String,
    /// Repo-relative path, forward slashes.
    pub path: String,
}

/// What a project file declares. `name` is a label, `path` is the file itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFacts {
    pub path: String,
    pub kind: ProjectKind,
    pub name: String,
    /// The directory holding the project file; empty at the repo root.
    pub root: String,
    /// Other project files this one references.
    pub references: Vec<ProjectReference>,
    /// External package or crate names. Never resolved to project files.
    pub packages: Vec<String>,
    /// Directories holding this project's code, repo-relative.
    pub roots: Vec<String>,
    /// `tsconfig.json` `compilerOptions.baseUrl`, repo-relative.
    pub base_url: Option<String>,
    /// `tsconfig.json` `compilerOptions.paths`, one entry per target, in file
    /// order. Targets are repo-relative and keep their `*`.
    pub aliases: Vec<PathAlias>,
}

/// One `paths` mapping of a `tsconfig.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathAlias {
    pub pattern: String,
    pub target: String,
}

/// Project facts for `path`, or `None` when it is not a project file.
pub fn project_facts(path: &str, src: &str) -> Option<ProjectFacts> {
    let kind = ProjectKind::of(path)?;
    Some(match kind {
        ProjectKind::Csproj => csproj(path, src),
        ProjectKind::Cargo => cargo(path, src),
        ProjectKind::Npm => package_json(path, src),
        ProjectKind::TsConfig => tsconfig_json(path, src),
        ProjectKind::PyProject => pyproject(path, src),
        ProjectKind::Pubspec => pubspec(path, src),
    })
}

fn base(path: &str, kind: ProjectKind) -> ProjectFacts {
    let root = dir_of(path);
    ProjectFacts {
        path: normalize(path),
        kind,
        name: String::new(),
        root: root.clone(),
        references: Vec::new(),
        packages: Vec::new(),
        roots: vec![root],
        base_url: None,
        aliases: Vec::new(),
    }
}

fn finish(mut facts: ProjectFacts, path: &str) -> ProjectFacts {
    if facts.name.is_empty() {
        facts.name = fallback_name(path);
    }
    facts
}

// ---------------------------------------------------------------- paths

fn normalize(path: &str) -> String {
    path.replace('\\', "/")
}

fn file_name(path: &str) -> String {
    normalize(path).rsplit('/').next().unwrap_or(path).to_string()
}

fn dir_of(path: &str) -> String {
    match normalize(path).rsplit_once('/') {
        Some((dir, _)) => dir.to_string(),
        None => String::new(),
    }
}

fn fallback_name(path: &str) -> String {
    let dir = dir_of(path);
    let last = dir.rsplit('/').next().unwrap_or("");
    if last.is_empty() { stem(path) } else { last.to_string() }
}

fn stem(path: &str) -> String {
    let file = file_name(path);
    file.rsplit_once('.').map_or(file.clone(), |(stem, _)| stem.to_string())
}

/// Join a path written inside a project file to the project's directory.
/// `..` is resolved lexically; a path that walks above the repo stays without
/// a leading `..` so it never matches a tracked file.
fn join(base: &str, rel: &str) -> String {
    let rel = normalize(rel.trim());
    if rel.starts_with('/') || rel.as_bytes().get(1) == Some(&b':') {
        return rel;
    }
    let mut parts: Vec<&str> = base.split('/').filter(|part| !part.is_empty()).collect();
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

// ---------------------------------------------------------------- csproj

fn csproj(path: &str, src: &str) -> ProjectFacts {
    let mut facts = base(path, ProjectKind::Csproj);
    if let Some(name) = tag_text(src, "AssemblyName").or_else(|| tag_text(src, "RootNamespace")) {
        facts.name = name;
    }
    for include in tag_attrs(src, "ProjectReference", "Include") {
        let path = join(&facts.root, &include);
        facts.references.push(ProjectReference {
            name: stem(&path),
            path,
        });
    }
    facts.packages = tag_attrs(src, "PackageReference", "Include");
    facts.packages.sort();
    facts.packages.dedup();
    finish(facts, path)
}

/// Text content of the first `<tag>text</tag>`.
fn tag_text(src: &str, tag: &str) -> Option<String> {
    let needle = format!("<{tag}");
    let mut from = 0;
    while let Some(offset) = src[from..].find(&needle) {
        let start = from + offset;
        let rest = &src[start + needle.len()..];
        if !rest.starts_with(|c: char| c.is_whitespace() || c == '>') {
            from = start + needle.len();
            continue;
        }
        let open_end = rest.find('>')?;
        let open = &rest[..open_end];
        if open.trim_end().ends_with('/') {
            from = start + needle.len();
            continue;
        }
        let body = &rest[open_end + 1..];
        let close = body.find("</")?;
        return Some(body[..close].trim().to_string());
    }
    None
}

/// `attr` values of every `<tag ...>` start element.
fn tag_attrs(src: &str, tag: &str, attr: &str) -> Vec<String> {
    let needle = format!("<{tag}");
    let mut values = Vec::new();
    let mut from = 0;
    while let Some(offset) = src[from..].find(&needle) {
        let start = from + offset;
        let rest = &src[start + needle.len()..];
        if !rest.starts_with(|c: char| c.is_whitespace() || c == '>') {
            from = start + needle.len();
            continue;
        }
        let Some(open_end) = rest.find('>') else { break };
        if let Some(value) = attr_value(&rest[..open_end], attr) {
            values.push(value);
        }
        from = start + needle.len() + open_end;
    }
    values
}

fn attr_value(tag_body: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=");
    let offset = tag_body.find(&needle)?;
    let after = tag_body[offset + needle.len()..].trim_start();
    let (quote, after) = match after.chars().next()? {
        q @ ('"' | '\'') => (Some(q), &after[1..]),
        _ => (None, after),
    };
    let end = match quote {
        Some(q) => after.find(q)?,
        None => after
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .unwrap_or(after.len()),
    };
    Some(after[..end].trim().to_string())
}

// ---------------------------------------------------------------- toml

/// One `key = value` line with the table it sits under.
struct Entry {
    table: String,
    key: String,
    value: String,
}

fn toml_entries(src: &str) -> Vec<Entry> {
    let lines: Vec<&str> = src.lines().collect();
    let mut entries = Vec::new();
    let mut table = String::new();
    let mut i = 0;
    while i < lines.len() {
        let line = strip_comment(lines[i]).trim().to_string();
        i += 1;
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            table = line.trim_matches(|c| c == '[' || c == ']').trim().to_string();
            continue;
        }
        let Some((key, value)) = split_top(&line, '=') else {
            continue;
        };
        let mut value = value.trim().to_string();
        while needs_more(&value) && i < lines.len() {
            let next = strip_comment(lines[i]).trim().to_string();
            i += 1;
            value.push(' ');
            value.push_str(&next);
        }
        entries.push(Entry {
            table: table.clone(),
            key: unquote(key.trim()),
            value,
        });
    }
    entries
}

fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' && q == '"' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '#' => return &line[..i],
                _ => {}
            },
        }
    }
    line
}

/// Split at the first `sep` outside quotes.
fn split_top(s: &str, sep: char) -> Option<(&str, &str)> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' && q == '"' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                c if c == sep => return Some((&s[..i], &s[i + c.len_utf8()..])),
                _ => {}
            },
        }
    }
    None
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// An array or inline table that continues on the next line.
fn needs_more(value: &str) -> bool {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for c in value.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                _ => {}
            },
        }
    }
    depth > 0
}

/// The strings of a TOML array, in order.
fn array_values(value: &str) -> Vec<String> {
    let inner = value.trim();
    let inner = inner.strip_prefix('[').unwrap_or(inner);
    let inner = inner.strip_suffix(']').unwrap_or(inner);
    inner
        .split(',')
        .map(|item| unquote(item.trim()))
        .filter(|item| !item.is_empty() && !item.starts_with('{'))
        .collect()
}

/// The `path` of an inline table such as `{ version = "1", path = "../lib" }`.
fn inline_path(value: &str) -> Option<String> {
    let inner = value.trim().strip_prefix('{')?.strip_suffix('}')?;
    for part in inner.split(',') {
        let (key, val) = split_top(part, '=')?;
        if key.trim() == "path" {
            return Some(unquote(val));
        }
    }
    None
}

fn is_dependency_table(table: &str) -> bool {
    table == "dependencies"
        || table == "dev-dependencies"
        || table == "build-dependencies"
        || table.ends_with(".dependencies")
        || table.ends_with(".dev-dependencies")
        || table.ends_with(".build-dependencies")
}

/// `dependencies.foo` and friends: the part after the last dot.
fn dependency_table_name(table: &str) -> Option<&str> {
    if is_dependency_table(table) {
        table.rsplit_once('.').map(|(_, name)| name)
    } else {
        None
    }
}

// ---------------------------------------------------------------- Cargo.toml

fn cargo(path: &str, src: &str) -> ProjectFacts {
    let mut facts = base(path, ProjectKind::Cargo);
    let entries = toml_entries(src);
    for entry in &entries {
        if entry.table == "package" && entry.key == "name" {
            facts.name = unquote(&entry.value);
        }
        if is_dependency_table(&entry.table) {
            // `serde.workspace = true` is a dotted key: the crate is the head.
            let krate = entry.key.split('.').next().unwrap_or(&entry.key);
            facts.packages.push(krate.to_string());
            if let Some(dep_path) = inline_path(&entry.value) {
                facts.references.push(reference(&facts.root, krate, &dep_path));
            }
        }
        if entry.table == "workspace" && entry.key == "members" {
            for member in array_values(&entry.value) {
                facts.roots.push(join(&facts.root, &member));
            }
        }
        if let Some(name) = dependency_table_name(&entry.table)
            && entry.key == "path"
        {
            facts
                .references
                .push(reference(&facts.root, name, &unquote(&entry.value)));
        }
    }
    finish(facts, path)
}

fn reference(base: &str, name: &str, rel: &str) -> ProjectReference {
    ProjectReference {
        name: name.to_string(),
        path: join(base, rel),
    }
}

// ---------------------------------------------------------------- pyproject

fn pyproject(path: &str, src: &str) -> ProjectFacts {
    let mut facts = base(path, ProjectKind::PyProject);
    let entries = toml_entries(src);
    for entry in &entries {
        match (entry.table.as_str(), entry.key.as_str()) {
            ("project", "name") | ("tool.poetry", "name") => facts.name = unquote(&entry.value),
            ("project", "dependencies") | ("project.optional-dependencies", _) => {
                for spec in array_values(&entry.value) {
                    facts.packages.push(package_name(&spec));
                }
            }
            ("tool.poetry.dependencies", key) if key != "python" => {
                facts.packages.push(key.to_string());
                if let Some(dep_path) = inline_path(&entry.value) {
                    facts.references.push(reference(&facts.root, key, &dep_path));
                }
            }
            ("tool.poetry.dev-dependencies", key) => facts.packages.push(key.to_string()),
            (table, key) if table.starts_with("tool.poetry.group.") && table.ends_with(".dependencies") => {
                facts.packages.push(key.to_string());
            }
            _ => {
                if let Some(name) = entry.table.strip_prefix("tool.poetry.dependencies.")
                    && entry.key == "path"
                {
                    facts
                        .references
                        .push(reference(&facts.root, name, &unquote(&entry.value)));
                }
            }
        }
    }
    facts.packages.sort();
    facts.packages.dedup();
    finish(facts, path)
}

/// `requests>=2,<3` and `pkg[extra]` name the package `requests` / `pkg`.
fn package_name(spec: &str) -> String {
    let spec = spec.trim();
    let end = spec
        .find(['<', '>', '=', '!', '~', ';', ' ', '[', '('])
        .unwrap_or(spec.len());
    spec[..end].to_string()
}

// ---------------------------------------------------------------- pubspec.yaml

/// `name`, `dependencies` and `dev_dependencies` of a Dart package. The file is read line by line
/// by indentation, which is all the keys it needs: no YAML parser. Code roots are `lib/` (first)
/// and `test/`. A `path:` dependency (also under `dependency_overrides`) is a reference to the
/// folder it names, so a monorepo's packages find each other.
fn pubspec(path: &str, src: &str) -> ProjectFacts {
    let mut facts = base(path, ProjectKind::Pubspec);
    facts.roots = vec![join(&facts.root, "lib"), join(&facts.root, "test")];
    let mut section = String::new();
    let mut entry_indent: Option<usize> = None;
    // The indent of the keys inside one dependency: only there `path:` names a folder (a
    // `git:` dependency has its own `path:` one level deeper, a folder of another repo).
    let mut field_indent: Option<usize> = None;
    let mut current: Option<String> = None;
    for raw in src.strip_prefix('\u{feff}').unwrap_or(src).lines() {
        let line = strip_comment(raw);
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "---" || trimmed.starts_with('-') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let (key, value) = (unquote(key.trim()), unquote(value.trim()));
        if key.is_empty() {
            continue;
        }
        if indent == 0 {
            section = key.clone();
            entry_indent = None;
            current = None;
            if key == "name" && !value.is_empty() {
                facts.name = value;
            }
            continue;
        }
        let overrides = section == "dependency_overrides";
        if !(overrides || section == "dependencies" || section == "dev_dependencies") {
            continue;
        }
        let first = *entry_indent.get_or_insert(indent);
        if indent <= first {
            if !overrides {
                facts.packages.push(key.clone());
            }
            // `pkg: { path: ../pkg }` on one line.
            if let Some(rel) = value
                .strip_prefix('{')
                .and_then(|inner| inner.split(',').find_map(|part| part.trim().strip_prefix("path:")))
            {
                let rel = unquote(rel.trim_end_matches('}').trim());
                facts.references.push(reference(&facts.root, &key, &rel));
            }
            current = Some(key);
            field_indent = None;
        } else {
            let field = *field_indent.get_or_insert(indent);
            if key == "path"
                && indent == field
                && !value.is_empty()
                && let Some(name) = &current
            {
                facts.references.push(reference(&facts.root, name, &value));
            }
        }
    }
    facts.packages.sort();
    facts.packages.dedup();
    finish(facts, path)
}

// ---------------------------------------------------------------- package.json

fn package_json(path: &str, src: &str) -> ProjectFacts {
    let mut facts = base(path, ProjectKind::Npm);
    let Some(value) = json_value(src) else {
        return finish(facts, path);
    };
    if let Some(name) = value
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
    {
        facts.name = name.to_string();
    }
    for table in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(map) = value.get(table).and_then(Value::as_object) {
            facts.packages.extend(map.keys().cloned());
        }
    }
    if let Some(workspaces) = value.get("workspaces") {
        let dirs = match workspaces {
            Value::Array(items) => items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>(),
            Value::Object(map) => map
                .get("packages")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        for dir in dirs {
            facts.roots.push(join(&facts.root, &dir));
        }
    }
    facts.packages.sort();
    facts.packages.dedup();
    finish(facts, path)
}

// ---------------------------------------------------------------- tsconfig.json

fn tsconfig_json(path: &str, src: &str) -> ProjectFacts {
    let mut facts = base(path, ProjectKind::TsConfig);
    let Some(value) = json_value(&strip_json_comments(src)) else {
        return finish(facts, path);
    };
    if let Some(items) = value.get("references").and_then(Value::as_array) {
        for item in items {
            let Some(rel) = item.get("path").and_then(Value::as_str) else {
                continue;
            };
            let referenced = join(&facts.root, rel);
            let name = stem(&referenced);
            facts.references.push(ProjectReference { name, path: referenced });
        }
    }
    let options = value.get("compilerOptions");
    if let Some(base) = options.and_then(|o| o.get("baseUrl")).and_then(Value::as_str) {
        facts.base_url = Some(join(&facts.root, base));
    }
    if let Some(paths) = options.and_then(|o| o.get("paths")).and_then(Value::as_object) {
        let anchor = facts.base_url.clone().unwrap_or_else(|| facts.root.clone());
        for (pattern, targets) in paths {
            for target in targets.as_array().into_iter().flatten().filter_map(Value::as_str) {
                facts.aliases.push(PathAlias {
                    pattern: pattern.clone(),
                    target: join(&anchor, target),
                });
            }
        }
    }
    let mut roots = Vec::new();
    for key in ["files", "include"] {
        if let Some(items) = value.get(key).and_then(Value::as_array) {
            for item in items.iter().filter_map(Value::as_str) {
                roots.push(join(&facts.root, item));
            }
        }
    }
    if !roots.is_empty() {
        facts.roots = roots;
    }
    finish(facts, path)
}

fn json_value(src: &str) -> Option<Value> {
    serde_json::from_str(src).ok()
}

/// Drop `//` and `/* */` comments, which `tsconfig.json` allows and
/// `serde_json` does not. String contents are kept as written.
fn strip_json_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
            continue;
        }
        if c == '/' {
            match chars.peek().copied() {
                Some('/') => {
                    for next in chars.by_ref() {
                        if next == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                }
                Some('*') => {
                    chars.next();
                    let mut prev = ' ';
                    for next in chars.by_ref() {
                        if prev == '*' && next == '/' {
                            break;
                        }
                        if next == '\n' {
                            out.push('\n');
                        }
                        prev = next;
                    }
                }
                _ => out.push(c),
            }
            continue;
        }
        out.push(c);
    }
    out
}
