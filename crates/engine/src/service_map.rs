//! `stone arch map`: which folder holds each service's code. A container or component with no `code`
//! is matched by name to the project files in the index (`.csproj`, `package.json`,
//! `pyproject.toml`, `Cargo.toml`); one match is proposed, several are listed for the owner to pick.
//! The shared projects the services reference are proposed as components of their own.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use carvestone_core::ItemType;
use carvestone_graph::{ProjectFacts, ProjectKind, project_facts};
use rusqlite::Connection;
use serde::Serialize;
use serde_norway::Value;

use crate::code::open;
use crate::code_rank::is_support_path;
use crate::create::{NewItem, Written};
use crate::edit::type_of;
use crate::fields::SetValue;
use crate::new_item::ascii_slug;
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::scan::escape_glob;

/// One service without code and the folder stone proposes for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceCode {
    pub id: String,
    pub title: String,
    /// The project folder named like the service, when exactly one is.
    pub folder: Option<String>,
    /// The project file that folder was found by.
    pub project: Option<String>,
    /// Every project folder named like the service, when more than one is.
    pub candidates: Vec<String>,
}

/// A shared project the services reference that no element names yet, and the component stone
/// proposes for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryCode {
    /// The id the new component gets: beside the first service that uses it.
    pub id: String,
    /// The project's own name.
    pub name: String,
    pub folder: String,
    /// The parent most of its services share.
    pub parent: Option<String>,
    /// The services that reference it, directly or through another library, in id order.
    pub used_by: Vec<String>,
}

/// Letters and digits only, lower case: `Freightline.Sync.Api`, `freightline-sync-api` and
/// `freightline_sync_api` are one name.
fn key(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The project files in the index that can hold a service or a library, in path order. A
/// tsconfig sits beside the package.json that names the project; the repo root is every
/// service's folder at once.
fn projects(repo: &Repo) -> Result<Vec<ProjectFacts>, EngineError> {
    let index = open(repo)?;
    projects_in(&repo.store.layout.root, index.conn())
}

/// `projects` over an index already open.
pub(crate) fn projects_in(root: &Path, conn: &Connection) -> Result<Vec<ProjectFacts>, EngineError> {
    let paths: Vec<String> = {
        let mut stmt = conn.prepare("SELECT path FROM files WHERE lang = 'project' ORDER BY path")?;
        stmt.query_map([], |row| row.get(0))?.collect::<Result<_, _>>()?
    };
    Ok(paths
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(root.join(&path)).ok()?;
            project_facts(&path, &text)
        })
        .filter(|facts| facts.kind != ProjectKind::TsConfig && !facts.root.is_empty())
        .collect())
}

/// The folders elements name as a whole (`<folder>/**`), as written in the globs.
fn placed(snapshot: &Snapshot) -> BTreeSet<String> {
    snapshot
        .entries
        .iter()
        .filter_map(|e| e.item.as_ref())
        .flat_map(|item| item.fields.code.iter())
        .filter_map(|glob| glob.trim().strip_suffix("/**").map(str::to_string))
        .collect()
}

/// Project folders by each name they go by: the project's own name and the folder's.
fn project_names(repo: &Repo) -> Result<BTreeMap<String, BTreeMap<String, String>>, EngineError> {
    // An element's whole-folder glob already places that folder.
    let placed = placed(&repo.snapshot()?);
    let mut names: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for facts in projects(repo)? {
        if placed.contains(&escape_glob(&facts.root)) {
            continue;
        }
        let path = facts.path.clone();
        let folder = facts.root.rsplit('/').next().unwrap_or(&facts.root);
        for name in [facts.name.as_str(), folder] {
            let name = key(name);
            if !name.is_empty() {
                names
                    .entry(name)
                    .or_default()
                    .entry(facts.root.clone())
                    .or_insert_with(|| path.clone());
            }
        }
    }
    Ok(names)
}

impl Repo {
    /// Every container and component without `code`, in id order, with the project folder named
    /// like its title or its file name.
    pub fn service_code(&self) -> Result<Vec<ServiceCode>, EngineError> {
        let names = project_names(self)?;
        let snapshot = self.snapshot()?;
        let mut out = Vec::new();
        for entry in &snapshot.entries {
            let Some(item) = &entry.item else { continue };
            if !matches!(item.item_type(), Some(ItemType::Container | ItemType::Component))
                || !item.fields.code.is_empty()
            {
                continue;
            }
            let title = item.fields.title.clone().unwrap_or_default();
            let stem = entry.id.as_str().rsplit('/').next().unwrap_or_default();
            let mut found: BTreeMap<&String, &String> = BTreeMap::new();
            for name in [key(&title), key(stem)] {
                if let Some(folders) = names.get(&name) {
                    found.extend(folders.iter());
                }
            }
            let single = (found.len() == 1).then(|| found.iter().next().map(|(f, p)| ((*f).clone(), (*p).clone())));
            let (folder, project) = single.flatten().unzip();
            out.push(ServiceCode {
                id: snapshot.label(entry),
                title,
                candidates: if found.len() > 1 {
                    found.keys().map(|f| (*f).clone()).collect()
                } else {
                    Vec::new()
                },
                folder,
                project,
            });
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    /// Sets `code: ['<folder>/**']` on each service named in `picks` (an item and a folder).
    /// A service that already has code is refused; it is the owner's to change by hand.
    pub fn set_service_code(&self, picks: &[(String, String)]) -> Result<Vec<Written>, EngineError> {
        let root = &self.store.layout.root;
        let mut out = Vec::new();
        for (input, folder) in picks {
            let folder = folder
                .trim()
                .trim_start_matches("./")
                .trim_end_matches('/')
                .replace('\\', "/");
            if folder.is_empty() || !root.join(&folder).is_dir() {
                return Err(EngineError::Invalid(format!(
                    "`{folder}` is not a folder under {}; name the service's folder relative to it",
                    root.display()
                )));
            }
            let glob = format!("{}/**", escape_glob(&folder));
            out.push(self.update(input, false, |_, entry, doc| {
                let kind = type_of(entry)?;
                if !matches!(kind, ItemType::Container | ItemType::Component) {
                    return Err(EngineError::Invalid(format!(
                        "{} is a {}; only containers and components hold code",
                        entry.path,
                        kind.as_str()
                    )));
                }
                if !doc.get_list("code").unwrap_or_default().is_empty() {
                    return Err(EngineError::Invalid(format!(
                        "{} already names its code; change its `code` list by hand",
                        entry.path
                    )));
                }
                doc.set("code", Value::Sequence(vec![Value::String(glob.clone())]))
                    .map_err(|e| EngineError::Invalid(format!("{}: {e}", entry.path)))
            })?);
        }
        Ok(out)
    }

    /// The shared projects the services reference, directly or through each other, that no
    /// element names as a whole, in id order. A service is a container or component whose
    /// `code` is one project folder; `pending` adds folders about to be saved (service, folder).
    /// Tests and benchmarks reference services, not the other way round, and are never proposed.
    pub fn library_code(&self, pending: &[(String, String)]) -> Result<Vec<LibraryCode>, EngineError> {
        let projects = projects(self)?;
        let snapshot = self.snapshot()?;
        let placed = placed(&snapshot);
        let roots: BTreeSet<String> = projects.iter().map(|p| escape_glob(&p.root)).collect();
        // (id, folder as written in a glob, parent)
        let mut services: Vec<(String, String, Option<String>)> = Vec::new();
        for entry in &snapshot.entries {
            let Some(item) = &entry.item else { continue };
            if !matches!(item.item_type(), Some(ItemType::Container | ItemType::Component)) {
                continue;
            }
            let id = snapshot.label(entry);
            let folder = pending
                .iter()
                .find(|(service, _)| *service == id)
                .map(|(_, folder)| escape_glob(folder.trim().trim_end_matches('/')))
                .or_else(|| match item.fields.code.as_slice() {
                    [glob] => glob.trim().strip_suffix("/**").map(str::to_string),
                    _ => None,
                });
            if let Some(folder) = folder.filter(|f| roots.contains(f)) {
                services.push((id, folder, item.fields.parent.clone()));
            }
        }
        let in_service = |root: &str| {
            let root = escape_glob(root);
            services
                .iter()
                .any(|(_, folder, _)| root == *folder || root.starts_with(&format!("{folder}/")))
        };
        let by_path: BTreeMap<&str, usize> = projects.iter().enumerate().map(|(i, p)| (p.path.as_str(), i)).collect();
        // Library project index -> the services that reach it.
        let mut users: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
        for (s, (_, folder, _)) in services.iter().enumerate() {
            let mut queue: Vec<usize> = (0..projects.len())
                .filter(|i| escape_glob(&projects[*i].root) == *folder)
                .collect();
            let mut seen: BTreeSet<usize> = queue.iter().copied().collect();
            while let Some(at) = queue.pop() {
                for reference in &projects[at].references {
                    let Some(&next) = by_path.get(reference.path.as_str()) else {
                        continue;
                    };
                    if !seen.insert(next) || in_service(&projects[next].root) {
                        continue;
                    }
                    users.entry(next).or_default().insert(s);
                    queue.push(next);
                }
            }
        }
        let mut out = Vec::new();
        for (library, by) in users {
            let facts = &projects[library];
            if placed.contains(&escape_glob(&facts.root)) || is_support_path(&format!("{}/-", facts.root)) {
                continue;
            }
            let Some(first) = by.iter().next().map(|s| &services[*s]) else {
                continue;
            };
            let dir = first.0.rsplit_once('/').map_or("arch", |(dir, _)| dir);
            let id = format!("{dir}/{}", ascii_slug(&facts.name));
            if snapshot.entries.iter().any(|e| e.id.as_str().eq_ignore_ascii_case(&id)) {
                continue;
            }
            let mut parents: BTreeMap<&str, usize> = BTreeMap::new();
            for s in &by {
                if let Some(parent) = &services[*s].2 {
                    *parents.entry(parent.as_str()).or_default() += 1;
                }
            }
            let parent = parents
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                .map(|(p, _)| p.to_string());
            out.push(LibraryCode {
                id,
                name: facts.name.clone(),
                folder: facts.root.clone(),
                parent,
                used_by: by.iter().map(|s| services[*s].0.clone()).collect(),
            });
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    /// Creates a component for each library, beside its first service, owning its folder.
    pub fn add_libraries(&self, libraries: &[LibraryCode]) -> Result<Vec<Written>, EngineError> {
        let mut out = Vec::new();
        for library in libraries {
            let mut set = Vec::new();
            if let Some(parent) = &library.parent {
                set.push(("parent".to_string(), SetValue::Text(parent.clone())));
            }
            set.push((
                "code".to_string(),
                SetValue::List(vec![format!("{}/**", escape_glob(&library.folder))]),
            ));
            let req = NewItem {
                type_name: "component".to_string(),
                title: library.name.clone(),
                slug: Some(library.id.strip_prefix("arch/").unwrap_or(&library.id).to_string()),
                set,
                sections: vec![(
                    "Responsibilities".to_string(),
                    format!(
                        "Shared library. Used by {} (project references).",
                        library.used_by.join(", ")
                    ),
                )],
                body: None,
                links: Vec::new(),
                left_out: Vec::new(),
            };
            out.push(self.create(&req, false)?);
        }
        Ok(out)
    }
}
