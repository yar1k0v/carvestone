//! `stone scan`: draft components for modules whose files no curated element owns.
//!
//! Drafts live in `stone/arch/scan/`. `.stone/scan-state.json` holds the blake3 of
//! the text scan last wrote to each one; a draft whose text no longer hashes to
//! that is curated by a human and is never written or deleted.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::Path;

use carvestone_core::{ItemId, ItemType};
use carvestone_store::Document;
use carvestone_store::store::item_from_document;
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use rusqlite::Connection;

use crate::code::open;
use crate::code_entries::{detect, targets_in};
use crate::code_index::{project_dir, root_prefixes, stored_project_dirs};
use crate::create::now;
use crate::index_file;
use crate::modules::{Module, stored as stored_modules};
use crate::owner::{Element, arch_elements, tracked};
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;

const STATE_FILE: &str = ".stone/scan-state.json";
/// Where drafts live, relative to `stone/`.
const SCAN_DIR: &str = "arch/scan";

#[derive(Debug, Clone, Default)]
pub struct ScanOpts {
    /// Report what would happen and write nothing.
    pub dry_run: bool,
    /// Delete drafts whose files are all owned elsewhere.
    pub prune: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanState {
    New,
    Changed,
    Unchanged,
    /// A human edited the draft; it is left alone.
    KeptEdited,
    /// No module needs the draft any more; `--prune` deletes it.
    Stale,
    Pruned,
}

impl ScanState {
    pub fn as_str(self) -> &'static str {
        match self {
            ScanState::New => "new",
            ScanState::Changed => "changed",
            ScanState::Unchanged => "unchanged",
            ScanState::KeptEdited => "kept",
            ScanState::Stale => "stale",
            ScanState::Pruned => "pruned",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanReport {
    /// Drafts created or rewritten (or that a dry run would write).
    pub written: usize,
    /// Every draft with its state, sorted by path (`arch/scan/<name>.md`, relative to `stone/`).
    pub entries: Vec<(String, ScanState)>,
}

impl ScanReport {
    pub fn count(&self, state: ScanState) -> usize {
        self.entries.iter().filter(|(_, s)| *s == state).count()
    }
}

struct Draft {
    text: String,
    /// The text still hashes to what scan last wrote.
    managed: bool,
    /// What the draft's `code` list covers.
    globs: GlobSet,
    created: Option<String>,
}

fn hash(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

fn read_text(path: &Path) -> Result<Option<String>, EngineError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(EngineError::Invalid(format!("cannot read {}: {e}", path.display()))),
    }
}

fn load_state(path: &Path) -> Result<BTreeMap<String, String>, EngineError> {
    match read_text(path)? {
        None => Ok(BTreeMap::new()),
        Some(text) => serde_json::from_str(&text).map_err(|e| {
            EngineError::Invalid(format!(
                "cannot read {STATE_FILE}: {e}; delete it to treat every draft as edited by a human"
            ))
        }),
    }
}

fn save_state(path: &Path, state: &BTreeMap<String, String>) -> Result<(), EngineError> {
    let mut text = serde_json::to_string_pretty(state).map_err(|e| EngineError::Invalid(e.to_string()))?;
    text.push('\n');
    if read_text(path)?.as_deref() == Some(text.as_str()) {
        return Ok(());
    }
    Ok(carvestone_store::write::write_atomic(path, &text)?)
}

/// A glob that does not compile covers nothing.
fn globs_of(patterns: &[String]) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        if let Ok(glob) = GlobBuilder::new(pattern.trim()).literal_separator(true).build() {
            builder.add(glob);
        }
    }
    builder.build().unwrap_or_else(|_| GlobSet::empty())
}

/// The files already in `stone/arch/scan/`, keyed `arch/scan/<name>.md`.
fn read_drafts(repo: &Repo, state: &BTreeMap<String, String>) -> Result<BTreeMap<String, Draft>, EngineError> {
    let dir = repo.store.layout.data_dir.join(SCAN_DIR);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(EngineError::Invalid(format!("cannot read {}: {e}", dir.display()))),
    };
    let mut drafts = BTreeMap::new();
    for entry in entries {
        let entry = entry.map_err(|e| EngineError::Invalid(format!("cannot read {}: {e}", dir.display())))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".md") {
            continue;
        }
        let key = format!("{SCAN_DIR}/{name}");
        let Some(text) = read_text(&entry.path())? else {
            continue;
        };
        let doc = Document::parse(&text).ok();
        drafts.insert(
            key.clone(),
            Draft {
                managed: state.get(&key).is_some_and(|h| *h == hash(&text)),
                globs: globs_of(&doc.as_ref().and_then(|d| d.get_list("code")).unwrap_or_default()),
                created: doc.as_ref().and_then(|d| d.get_str("created")),
                text,
            },
        );
    }
    Ok(drafts)
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    if out.is_empty() {
        "x".to_string()
    } else {
        out.to_string()
    }
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn tech_of(lang: &str) -> &'static str {
    match lang {
        "csharp" => "C#",
        "rust" => "Rust",
        "typescript" => "TypeScript",
        "javascript" => "JavaScript",
        "python" => "Python",
        "dart" => "Dart",
        _ => "unknown",
    }
}

fn list(items: impl IntoIterator<Item = String>) -> String {
    format!(
        "[{}]",
        items.into_iter().map(|i| quote(&i)).collect::<Vec<_>>().join(", ")
    )
}

/// What one draft stands for: a module with files no curated element owns, or a whole project
/// folder (a repo below the root) that nothing curated owns at all.
struct Subject {
    title: String,
    /// The draft's file name without the folder and `.md`.
    stem: String,
    /// The files the draft covers.
    files: Vec<String>,
    /// What the draft's `code` list says: the files, or one folder glob for a project.
    code: Vec<String>,
    /// The sentence under Responsibilities.
    summary: String,
    /// The system item a project draft sits under.
    parent: Option<String>,
}

impl Subject {
    fn module(module: &Module, unowned: Vec<String>) -> Subject {
        let h6: String = module
            .files
            .first()
            .map(|path| hash(path).chars().take(6).collect())
            .unwrap_or_default();
        Subject {
            title: format!("{}: {}", module.project, module.label),
            stem: format!("{}-{}-{h6}", slug(&module.project), slug(&module.label)),
            summary: format!(
                "Draft from `stone scan`: {} files under `{}` in project {}. Say what they are for, curate `code`, `entry` and `uses`, then change the status.",
                unowned.len(),
                module.label,
                module.project
            ),
            code: unowned.clone(),
            files: unowned,
            parent: None,
        }
    }

    /// The files of a code root nothing else covers. When that is the whole folder it is the
    /// project draft; otherwise the draft lists just those files.
    fn leftover(dir: &str, files: Vec<String>, code: Vec<String>, parent: Option<&str>) -> Subject {
        if code == [format!("{dir}/**")] {
            return Subject::project(dir, files, parent);
        }
        Subject {
            title: dir.rsplit('/').next().unwrap_or(dir).to_string(),
            stem: slug(dir),
            summary: format!(
                "Draft from `stone scan`: {} files under `{dir}/` that no component owns yet (scripts, config, files stone cannot parse). Say what they are for, then change the status.",
                files.len()
            ),
            code,
            files,
            parent: parent.map(str::to_string),
        }
    }

    fn project(dir: &str, files: Vec<String>, parent: Option<&str>) -> Subject {
        Subject {
            title: dir.rsplit('/').next().unwrap_or(dir).to_string(),
            stem: slug(dir),
            summary: format!(
                "Draft from `stone scan`: {} files under `{dir}/`, a project no component owns yet. Say what it is for, split it into components when it is big, curate `entry` and `uses`, then change the status.",
                files.len()
            ),
            code: vec![format!("{dir}/**")],
            files,
            parent: parent.map(str::to_string),
        }
    }
}

/// The draft's full text. A pure function of the subject, so a second run writes nothing.
fn render(subject: &Subject, technology: &str, entries: &[String], uses: &BTreeSet<String>, date: &str) -> String {
    // Only a draft that has entries gets the key, and only a project draft a parent, so other
    // drafts keep their text.
    let entry = if entries.is_empty() {
        String::new()
    } else {
        format!("entry: {}\n", list(entries.iter().cloned()))
    };
    let parent = subject
        .parent
        .as_ref()
        .map_or_else(String::new, |p| format!("parent: {}\n", quote(p)));
    // A colon inside `{ .. }` needs quotes; a plain day keeps its old text.
    let at = if date.contains(':') {
        format!("'{date}'")
    } else {
        date.to_string()
    };
    format!(
        "---\ntype: component\ntitle: {title}\nstatus: draft\n{parent}technology: {technology}\ncode: {code}\n{entry}uses: {uses}\ncreated: {date}\ngenerated: {{ by: 'process:stone-scan', at: {at} }}\n---\n\n## Responsibilities\n\n{summary}\n",
        title = quote(&subject.title),
        code = list(subject.code.iter().cloned()),
        uses = list(uses.iter().cloned()),
        summary = subject.summary,
    )
}

/// `<stem>.md` in the scan folder, with `-2`, `-3`, ... when the name is taken.
fn new_key(stem: &str, taken: &BTreeSet<String>) -> String {
    let base = format!("{SCAN_DIR}/{stem}");
    let mut key = format!("{base}.md");
    let mut n = 2;
    while taken.contains(&key) {
        key = format!("{base}-{n}.md");
        n += 1;
    }
    key
}

/// The system item every project draft sits under, when the store has exactly one.
fn single_system(snapshot: &Snapshot) -> Option<String> {
    let mut systems = snapshot
        .entries
        .iter()
        .filter(|e| e.item.as_ref().and_then(|i| i.item_type()) == Some(ItemType::System));
    let first = systems.next()?;
    systems.next().is_none().then(|| first.id.as_str().to_string())
}

/// `*?[]{}` in a path would read as glob syntax.
pub(crate) fn escape_glob(text: &str) -> String {
    text.chars()
        .map(|c| {
            if matches!(c, '*' | '?' | '[' | ']' | '{' | '}') {
                format!("[{c}]")
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// Globs that match exactly `left` among `all` (the tracked files under `dir`): `d/**` for a
/// folder whose every file is left over, `d/*` when all of a folder's own files are, else the
/// file paths.
fn leftover_globs(dir: &str, all: &[String], left: &BTreeSet<&str>) -> Vec<String> {
    #[derive(Default)]
    struct Folder {
        files: Vec<(String, bool)>,
        children: BTreeSet<String>,
        total: usize,
        left: usize,
    }
    fn emit(path: &str, folders: &BTreeMap<String, Folder>, out: &mut Vec<String>) {
        let folder = &folders[path];
        if folder.left == 0 {
            return;
        }
        if folder.left == folder.total {
            out.push(format!("{}/**", escape_glob(path)));
            return;
        }
        let own_left: Vec<&str> = folder
            .files
            .iter()
            .filter(|(_, l)| *l)
            .map(|(f, _)| f.as_str())
            .collect();
        if !folder.files.is_empty() && own_left.len() == folder.files.len() {
            out.push(format!("{}/*", escape_glob(path)));
        } else {
            out.extend(own_left.into_iter().map(escape_glob));
        }
        for child in &folder.children {
            emit(child, folders, out);
        }
    }
    let mut folders: BTreeMap<String, Folder> = BTreeMap::new();
    folders.entry(dir.to_string()).or_default();
    for file in all {
        let is_left = left.contains(file.as_str());
        let mut parent = file.rsplit_once('/').map_or("", |(p, _)| p).to_string();
        folders
            .entry(parent.clone())
            .or_default()
            .files
            .push((file.clone(), is_left));
        loop {
            let folder = folders.entry(parent.clone()).or_default();
            folder.total += 1;
            folder.left += usize::from(is_left);
            if parent == dir {
                break;
            }
            let up = parent.rsplit_once('/').map_or("", |(p, _)| p).to_string();
            folders.entry(up.clone()).or_default().children.insert(parent);
            parent = up;
        }
    }
    let mut out = Vec::new();
    emit(dir, &folders, &mut out);
    out
}

/// What to draft: one component for each project folder (a repo below the root) none of whose
/// files a curated element owns, for every other module its files no curated element owns, and
/// for each code root the files nothing above covers. `tracked` is every file git lists under
/// the code roots.
fn subjects_of<'a>(
    snapshot: &Snapshot,
    conn: &Connection,
    modules: &[Module],
    curated: &[Element],
    indexed: impl Iterator<Item = &'a String>,
    roots: &[String],
    tracked: &[String],
) -> Result<Vec<Subject>, EngineError> {
    let owned = |file: &str| curated.iter().any(|e| e.globs.is_match(file));
    let dirs = stored_project_dirs(conn)?;
    let mut by_project: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for file in indexed {
        let dir = project_dir(&dirs, file);
        if !dir.is_empty() {
            by_project.entry(dir).or_default().push(file.clone());
        }
    }
    let system = single_system(snapshot);
    let mut subjects = Vec::new();
    let mut whole: BTreeSet<&str> = BTreeSet::new();
    for (dir, mut files) in by_project {
        if files.iter().all(|f| !owned(f)) {
            files.sort();
            whole.insert(dir);
            subjects.push(Subject::project(dir, files, system.as_deref()));
        }
    }
    for module in modules {
        let unowned: Vec<String> = module.files.iter().filter(|f| !owned(f)).cloned().collect();
        let in_whole = module
            .files
            .first()
            .is_some_and(|f| whole.contains(project_dir(&dirs, f)));
        if !unowned.is_empty() && !in_whole {
            subjects.push(Subject::module(module, unowned));
        }
    }
    // Every tracked file of a code root needs exactly one owner, so the files nothing above
    // covers (SQL, YAML, Terraform, Kotlin: nothing stone parses) go to one draft per root. Its
    // globs name only those files, because a second glob over a file is a second owner.
    let mut covers: Vec<GlobSet> = subjects.iter().map(|s| globs_of(&s.code)).collect();
    let mut rooted: BTreeSet<String> = BTreeSet::new();
    for prefix in root_prefixes(roots) {
        let dir = prefix.trim_end_matches('/');
        if dir.is_empty() || !rooted.insert(prefix.clone()) {
            continue;
        }
        let under: Vec<String> = tracked.iter().filter(|f| f.starts_with(&prefix)).cloned().collect();
        let left: Vec<String> = under
            .iter()
            .filter(|f| !owned(f) && !covers.iter().any(|c| c.is_match(f.as_str())))
            .cloned()
            .collect();
        if left.is_empty() {
            continue;
        }
        let code = leftover_globs(dir, &under, &left.iter().map(String::as_str).collect());
        covers.push(globs_of(&code));
        subjects.push(Subject::leftover(dir, left, code, system.as_deref()));
    }
    Ok(subjects)
}

/// Writes or refreshes a draft for every module with files no curated element
/// owns. The store lock is taken first and everything is read under it; the
/// state file is saved before the lock is released.
pub fn scan(repo: &Repo, opts: &ScanOpts) -> Result<ScanReport, EngineError> {
    let lock = if opts.dry_run { None } else { Some(repo.store.lock()?) };
    let layout = &repo.store.layout;
    let state_path = layout.root.join(STATE_FILE);
    let mut state = load_state(&state_path)?;
    let drafts = read_drafts(repo, &state)?;

    let index = open(repo)?;
    let conn = index.conn();
    let modules = stored_modules(conn)?;
    let snapshot = repo.snapshot()?;
    // Drafts own nothing here: a module's files are unowned unless a component outside
    // `arch/scan/` claims them. An edited draft still takes its module through the match below.
    let curated: Vec<Element> = arch_elements(&snapshot, &mut Vec::new())
        .into_iter()
        .filter(|e| !drafts.contains_key(&format!("{}.md", e.id)))
        .collect();

    let langs: HashMap<String, String> = {
        let mut stmt = conn.prepare("SELECT path, lang FROM files WHERE lang IS NOT NULL AND lang <> 'project'")?;
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?
    };
    let mut depends: HashMap<String, BTreeSet<String>> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT sf.path, df.path FROM edges e \
             JOIN symbols s ON s.id = e.src_id JOIN files sf ON sf.id = s.file_id \
             JOIN symbols d ON d.id = e.dst_id JOIN files df ON df.id = d.file_id \
             WHERE e.dst_id IS NOT NULL AND sf.id <> df.id",
        )?;
        for row in stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
            let (src, dst) = row?;
            depends.entry(src).or_default().insert(dst);
        }
    }

    let found = detect(conn, &layout.root, &|_| true)?;
    let today = now();
    let mut claimed: BTreeSet<String> = BTreeSet::new();
    let mut taken: BTreeSet<String> = drafts.keys().cloned().collect();
    let mut entries: Vec<(String, ScanState)> = Vec::new();
    let roots = &repo.store.config.code_roots;
    let listed = tracked(repo, roots).unwrap_or_default();
    let subjects = subjects_of(&snapshot, conn, &modules, &curated, langs.keys(), roots, &listed)?;
    for subject in &subjects {
        let unowned = &subject.files;
        // The unclaimed draft sharing most of the subject's files, if that is more than half of them.
        let matched = drafts
            .iter()
            .filter(|(key, _)| !claimed.contains(*key))
            .map(|(key, d)| (unowned.iter().filter(|f| d.globs.is_match(f.as_str())).count(), key))
            .filter(|(overlap, _)| overlap * 2 > unowned.len())
            .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(a.1)))
            .map(|(_, key)| key.clone());
        let existing = matched.as_ref().and_then(|key| drafts.get(key));
        let key = matched.unwrap_or_else(|| new_key(&subject.stem, &taken));
        claimed.insert(key.clone());
        taken.insert(key.clone());

        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for file in unowned {
            *counts.entry(langs.get(file).map_or("", String::as_str)).or_default() += 1;
        }
        let lang = counts
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
            .map_or("", |(lang, _)| *lang);
        let uses: BTreeSet<String> = unowned
            .iter()
            .filter_map(|file| depends.get(file))
            .flatten()
            .flat_map(|dst| curated.iter().filter(|e| e.owns(dst.as_str())))
            .map(|e| e.id.clone())
            .collect();
        let date = existing
            .and_then(|d| d.created.clone())
            .unwrap_or_else(|| today.clone());
        let files: BTreeSet<&str> = unowned.iter().map(|f| f.as_str()).collect();
        let starts = targets_in(&found, &files);
        let text = render(subject, tech_of(lang), &starts, &uses, &date);

        let outcome = match existing {
            None => ScanState::New,
            Some(d) if d.text == text => ScanState::Unchanged,
            Some(d) if d.managed => ScanState::Changed,
            Some(_) => ScanState::KeptEdited,
        };
        if let Some(lock) = &lock {
            match outcome {
                ScanState::New | ScanState::Changed => {
                    let id = ItemId::from_path(&key);
                    repo.store.save(lock, &id, &text)?;
                    if let Ok(doc) = Document::parse(&text) {
                        let item = item_from_document(id.clone(), &doc);
                        repo.sync_index(lock, |index| index_file::put(index, &snapshot.scheme, &id, &item))?;
                    }
                    state.insert(key.clone(), hash(&text));
                }
                ScanState::Unchanged => {
                    state.insert(key.clone(), hash(&text));
                }
                _ => {}
            }
        }
        entries.push((key, outcome));
    }

    // Unedited drafts no module claims any more.
    for (key, draft) in &drafts {
        if claimed.contains(key) || !draft.managed {
            continue;
        }
        if opts.prune {
            if let Some(lock) = &lock {
                let path = layout.data_dir.join(key);
                fs::remove_file(&path)
                    .map_err(|e| EngineError::Invalid(format!("cannot delete {}: {e}", path.display())))?;
                repo.sync_index(lock, |index| index_file::drop_line(index, &ItemId::from_path(key)))?;
                state.remove(key);
            }
            entries.push((key.clone(), ScanState::Pruned));
        } else {
            entries.push((key.clone(), ScanState::Stale));
        }
    }

    if lock.is_some() {
        state.retain(|key, _| drafts.contains_key(key) || claimed.contains(key));
        save_state(&state_path, &state)?;
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let written = entries
        .iter()
        .filter(|(_, s)| matches!(s, ScanState::New | ScanState::Changed))
        .count();
    Ok(ScanReport { written, entries })
}
