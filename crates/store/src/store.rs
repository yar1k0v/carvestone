//! The store on disk: open, scan, build items from documents.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use carvestone_core::{Fields, Item, ItemId, LIST_KEYS, SCALAR_KEYS, sections};

use crate::config::Config;
use crate::frontmatter::{Document, FrontmatterError};
use crate::layout::Layout;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("no stone/ found; run `stone init`")]
    NoStore,
    #[error("{path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("invalid stone/config.yml: {0}")]
    Config(String),
    #[error("{0} already exists")]
    Exists(String),
    #[error("{0} is busy; close whatever holds it and retry")]
    Busy(String),
}

#[derive(Debug, Clone)]
pub struct Store {
    pub layout: Layout,
    pub config: Config,
}

/// One markdown file under `stone/`, parsed as far as it will go.
#[derive(Debug)]
pub struct Entry {
    pub path: String,
    pub id: ItemId,
    pub text: String,
    pub doc: Result<Document, FrontmatterError>,
    pub item: Option<Item>,
}

/// An item file as the disk shows it, before it is read.
#[derive(Debug, Clone)]
pub struct FileStat {
    /// Repo-relative with `/`, as `Entry::path`.
    pub path: String,
    /// Relative to `stone/`.
    pub rel: String,
    pub abs: PathBuf,
    pub mtime_ns: i64,
    pub size: u64,
    /// A file under a doc root: plain markdown that stone reads and never writes. `path` is
    /// root-relative and `rel` equals it.
    pub plain: bool,
}

impl Store {
    pub fn open(start: &Path) -> Result<Store, StoreError> {
        // Absolute but not canonical: a canonical path carries a verbatim prefix on Windows.
        let start = std::path::absolute(start).map_err(|source| StoreError::Io {
            path: start.display().to_string(),
            source,
        })?;
        let layout = Layout::discover(&start).ok_or(StoreError::NoStore)?;
        let config = Config::load(&layout.data_dir.join("config.yml"))?;
        Ok(Store { layout, config })
    }

    pub fn scan(&self) -> Result<Vec<Entry>, StoreError> {
        let mut entries = Vec::new();
        for file in self.stat_files()? {
            entries.push(self.read_entry(&file)?);
        }
        Ok(entries)
    }

    /// Every item file with its mtime and size, sorted by path, without reading any.
    pub fn stat_files(&self) -> Result<Vec<FileStat>, StoreError> {
        let mut files = Vec::new();
        for (rel, abs, meta) in self.walk()? {
            let name = rel.rsplit('/').next().unwrap_or(&rel);
            if is_reserved(name) {
                continue;
            }
            files.push(file_stat(format!("stone/{rel}"), rel, abs, &meta, false));
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    /// Every markdown file under the doc roots, and every project file (`README.md`, `CLAUDE.md`,
    /// `AGENTS.md`) in the root and the code roots, with its mtime and size, sorted by
    /// root-relative path. A folder that is not there is skipped, so a clone without it still works.
    pub fn stat_docs(&self) -> Result<Vec<FileStat>, StoreError> {
        let mut files: BTreeMap<String, FileStat> = BTreeMap::new();
        for folder in self.config.doc_folders() {
            let dir = self.layout.root.join(&folder);
            if !dir.is_dir() {
                continue;
            }
            let mut found = Vec::new();
            walk_dir(&dir, &folder, true, &mut found)?;
            for (rel, abs, meta) in found {
                files
                    .entry(rel.clone())
                    .or_insert_with(|| file_stat(rel.clone(), rel, abs, &meta, true));
            }
        }
        for folder in self.config.project_folders() {
            for (rel, abs, meta) in project_files(&self.layout.root, &folder)? {
                files
                    .entry(rel.clone())
                    .or_insert_with(|| file_stat(rel.clone(), rel, abs, &meta, true));
            }
        }
        Ok(files.into_values().collect())
    }

    /// The one doc-root file at root-relative `rel`, stat the way `stat_docs` would list it:
    /// `None` when it is outside the doc roots, in a hidden or `node_modules` folder, a
    /// symlink, not `.md` or not there.
    pub fn stat_doc(&self, rel: &str) -> Result<Option<FileStat>, StoreError> {
        let Some(folder) = self.config.doc_root_of(rel) else {
            return Ok(None);
        };
        let below = rel.strip_prefix(&folder).unwrap_or(rel).trim_start_matches('/');
        let mut parts: Vec<&str> = below.split('/').collect();
        let name = parts.pop().unwrap_or_default();
        if !name.ends_with(".md") || parts.iter().any(|p| p.starts_with('.') || *p == "node_modules") {
            return Ok(None);
        }
        let abs = self.layout.root.join(rel);
        match std::fs::symlink_metadata(&abs) {
            Ok(meta) if meta.is_file() => Ok(Some(file_stat(rel.to_string(), rel.to_string(), abs, &meta, true))),
            Ok(_) => Ok(None),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(io_err(&abs, err)),
        }
    }

    /// Every file under the doc roots, read.
    pub fn scan_docs(&self) -> Result<Vec<Entry>, StoreError> {
        self.stat_docs()?.iter().map(|file| self.read_entry(file)).collect()
    }

    pub fn read_entry(&self, file: &FileStat) -> Result<Entry, StoreError> {
        let text = read_lossy(&file.abs)?;
        if file.plain {
            return Ok(plain_entry(&file.path, text));
        }
        let id = ItemId::from_path(&file.rel);
        let doc = Document::parse(&text);
        let item = doc.as_ref().ok().map(|d| item_from_document(id.clone(), d));
        Ok(Entry {
            path: file.path.clone(),
            id,
            text,
            doc,
            item,
        })
    }

    pub fn scan_reserved(&self) -> Result<Vec<(String, String)>, StoreError> {
        let mut out = Vec::new();
        for (rel, abs, _) in self.walk()? {
            let name = rel.rsplit('/').next().unwrap_or(&rel);
            if is_reserved(name) {
                out.push((format!("stone/{rel}"), read_lossy(&abs)?));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    /// Every `.md` file under `stone/` outside the file store, as (relative path with `/`,
    /// absolute path, metadata). The metadata comes with the directory listing, so a
    /// walk costs no extra call per file on Windows.
    fn walk(&self) -> Result<Vec<WalkEntry>, StoreError> {
        let mut out = Vec::new();
        walk_dir(&self.layout.data_dir, "", false, &mut out)?;
        Ok(out)
    }
}

fn file_stat(path: String, rel: String, abs: PathBuf, meta: &std::fs::Metadata, plain: bool) -> FileStat {
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as i64);
    FileStat {
        path,
        rel,
        abs,
        mtime_ns,
        size: meta.len(),
        plain,
    }
}

/// A doc-root file as an entry: its id is its root-relative path. Front matter is used when it
/// parses (title, description, tags) and ignored when it does not; every file has an item, so
/// nothing about a plain file is ever a problem.
fn plain_entry(path: &str, text: String) -> Entry {
    let id = ItemId::from_file(path);
    let doc = Document::parse(&text);
    let (body, body_line) = match &doc {
        Ok(d) => (d.body.clone(), d.body_line()),
        Err(_) => (text.clone(), 1),
    };
    let mut fields = Fields::default();
    if let Ok(d) = &doc {
        fields.title = d.get_str("title");
        fields.description = d.get_str("description");
        fields.tags = d.get_list("tags").unwrap_or_default();
    }
    if fields.title.as_deref().is_none_or(|t| t.trim().is_empty()) {
        let stem = path.rsplit('/').next().unwrap_or(path);
        fields.title = Some(
            sections(&body, body_line)
                .into_iter()
                .find(|s| s.level == 1)
                .map(|s| s.title)
                .unwrap_or_else(|| stem.strip_suffix(".md").unwrap_or(stem).to_string()),
        );
    }
    let item = Item {
        id: id.clone(),
        type_name: None,
        fields,
        body,
        body_line,
    };
    Entry {
        path: path.to_string(),
        id,
        text,
        doc,
        item: Some(item),
    }
}

fn is_reserved(name: &str) -> bool {
    name == "index.md" || name == "log.md"
}

fn io_err(path: &Path, source: std::io::Error) -> StoreError {
    StoreError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// Lossy so one bad byte does not hide the rest of the store.
fn read_lossy(path: &Path) -> Result<String, StoreError> {
    let bytes = std::fs::read(path).map_err(|e| io_err(path, e))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

type WalkEntry = (String, PathBuf, std::fs::Metadata);

/// The project files directly in root-relative `folder`, spelled as on disk; none when the folder
/// is not there.
fn project_files(root: &Path, folder: &str) -> Result<Vec<WalkEntry>, StoreError> {
    let dir = if folder.is_empty() {
        root.to_path_buf()
    } else {
        root.join(folder)
    };
    let Ok(listing) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for entry in listing {
        let entry = entry.map_err(|e| io_err(&dir, e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !crate::config::PROJECT_FILES
            .iter()
            .any(|f| f.eq_ignore_ascii_case(&name))
        {
            continue;
        }
        let path = entry.path();
        // `file_type` does not follow symlinks, so they are skipped.
        if !entry.file_type().map_err(|e| io_err(&path, e))?.is_file() {
            continue;
        }
        let meta = entry.metadata().map_err(|e| io_err(&path, e))?;
        let rel = if folder.is_empty() {
            name
        } else {
            format!("{folder}/{name}")
        };
        out.push((rel, path, meta));
    }
    Ok(out)
}

/// `plain` walks a doc root: hidden folders and `node_modules` are skipped, every `.md` counts.
/// Otherwise it walks `stone/`: only the file store is skipped.
fn walk_dir(dir: &Path, prefix: &str, plain: bool, out: &mut Vec<WalkEntry>) -> Result<(), StoreError> {
    for entry in std::fs::read_dir(dir).map_err(|e| io_err(dir, e))? {
        let entry = entry.map_err(|e| io_err(dir, e))?;
        let path = entry.path();
        // `file_type` does not follow symlinks, so they are skipped.
        let kind = entry.file_type().map_err(|e| io_err(&path, e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if kind.is_dir() {
            let skip = if plain {
                name.starts_with('.') || name == "node_modules"
            } else {
                prefix.is_empty() && name == "files"
            };
            if skip {
                continue;
            }
            walk_dir(&path, &rel, plain, out)?;
        } else if kind.is_file() && name.ends_with(".md") {
            let meta = entry.metadata().map_err(|e| io_err(&path, e))?;
            out.push((rel, path, meta));
        }
    }
    Ok(())
}

pub fn item_from_document(id: ItemId, doc: &Document) -> Item {
    let mut fields = Fields::default();
    for key in SCALAR_KEYS {
        if let Some(slot) = fields.scalar_mut(key) {
            *slot = doc.get_str(key);
        }
    }
    for key in LIST_KEYS {
        if let Some(slot) = fields.list_mut(key) {
            *slot = doc.get_list(key).unwrap_or_default();
        }
    }
    fields.generated = doc.get_stamps("generated").ok().and_then(|s| s.into_iter().next());
    fields.verified = doc.get_stamps("verified").unwrap_or_default();
    Item {
        id,
        type_name: doc.get_str("type"),
        fields,
        body: doc.body.clone(),
        body_line: doc.body_line(),
    }
}
