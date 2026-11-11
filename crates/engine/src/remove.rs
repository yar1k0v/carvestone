//! `stone task remove` and `stone delete`: delete an item when nothing links to it (and, for a task, no commit
//! names it), or with `--force` anyway.

use carvestone_core::ItemType;
use serde::Serialize;

use crate::git_cache::index_with_history;
use crate::renumber::refers;
use crate::repo::{EngineError, Repo};

/// What a remove did, or for a dry run would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Removed {
    pub id: String,
    pub path: String,
    /// Items whose links this left pointing at nothing; only a forced delete has any.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub broken: Vec<String>,
}

/// Which command asked: they share the checks and differ in what they accept and how they refuse.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Way {
    /// `stone task remove`: tasks only, nothing is forced.
    Remove,
    /// `stone delete`: any item type; `force` deletes through links and commit trailers.
    Delete { force: bool },
}

/// At most this many names go into a refusal.
const NAMED: usize = 5;

fn few(names: &[String]) -> String {
    let mut shown = names.iter().take(NAMED).cloned().collect::<Vec<_>>().join(", ");
    if names.len() > NAMED {
        shown.push_str(&format!(" and {} more", names.len() - NAMED));
    }
    shown
}

impl Repo {
    /// Deletes the task `input` when no other item links to it and no commit has it in a `Refs` trailer.
    /// A refusal says why and points to `stone task move <id> archived`. A dry run checks and deletes nothing.
    pub fn remove_task(&self, input: &str, dry_run: bool) -> Result<Removed, EngineError> {
        self.drop_item(input, dry_run, Way::Remove)
    }

    /// Deletes the item `input`, of any type. Without `force` it is refused while another item links to it
    /// (a task also while a commit names it); with `force` it goes and the result lists the items whose
    /// links it leaves broken. A dry run checks and deletes nothing.
    pub fn delete_item(&self, input: &str, dry_run: bool, force: bool) -> Result<Removed, EngineError> {
        self.drop_item(input, dry_run, Way::Delete { force })
    }

    fn drop_item(&self, input: &str, dry_run: bool, way: Way) -> Result<Removed, EngineError> {
        self.refuse_doc_file(input, false)?;
        // The lock comes first and covers the checks and the delete, so a link added in between cannot be lost.
        let lock = if dry_run { None } else { Some(self.store.lock()?) };
        let snapshot = self.snapshot()?;
        let (entry, anchor) = snapshot
            .resolve(input)
            .map_err(|err| self.refuse_doc_file(input, true).err().unwrap_or(err))?;
        if anchor.is_some() {
            return Err(EngineError::Invalid(format!(
                "`{input}` names a section; pass the item id to remove the whole item"
            )));
        }
        let label = snapshot.label(entry);
        let is_task = entry.item.as_ref().and_then(|i| i.item_type()) == Some(ItemType::Task);
        if way == Way::Remove && !is_task {
            return Err(EngineError::NotATask { id: label });
        }
        let force = way == Way::Delete { force: true };
        let archive = format!("archive it instead with `stone task move {label} archived`");
        let anyway = format!("delete it anyway with `stone delete {label} --force`");
        let (verb, way_out) = match (way, is_task) {
            (Way::Remove, _) => ("removing", archive),
            (_, true) => ("deleting", format!("{archive}, or {anyway}")),
            (_, false) => ("deleting", format!("change those links first, or {anyway}")),
        };

        let scheme = &snapshot.scheme;
        let old = match scheme.number(&entry.id) {
            Some((kind, n)) => scheme.numbered_id(kind, n),
            None => entry.id.clone(),
        };
        let mut linked: Vec<String> = Vec::new();
        let mut unreadable: Vec<String> = Vec::new();
        for other in snapshot.entries.iter().filter(|e| e.id != entry.id) {
            match &other.doc {
                Ok(doc) if refers(scheme, doc, &other.id, &old) => linked.push(snapshot.label(other)),
                Ok(_) => {}
                Err(_) => unreadable.push(other.path.clone()),
            }
        }
        linked.sort();
        if !linked.is_empty() && !force {
            return Err(EngineError::Invalid(format!(
                "{label} is linked from {}, so {verb} it would leave broken links; {way_out}",
                few(&linked)
            )));
        }
        if !unreadable.is_empty() && !force {
            unreadable.sort();
            return Err(EngineError::Invalid(format!(
                "{} has broken frontmatter, so stone cannot tell whether it links to {label}; fix it (see `stone check`) or {way_out}",
                few(&unreadable)
            )));
        }

        if is_task && !force {
            let commits = self.commits_naming(&entry.id, &label)?;
            if !commits.is_empty() {
                return Err(EngineError::Invalid(format!(
                    "{label} is named in commit{} {}, so it is part of the history; {way_out}",
                    if commits.len() == 1 { "" } else { "s" },
                    few(&commits)
                )));
            }
        }

        if let Some(lock) = &lock {
            self.store.remove(lock, &entry.id)?;
        }
        Ok(Removed {
            id: label,
            path: entry.path.clone(),
            broken: linked,
        })
    }

    /// Short shas of the commits whose `Refs` trailer names the task. When the history cannot be read in a
    /// checkout that has commits, the answer is an error: removing must not guess.
    fn commits_naming(&self, id: &carvestone_core::ItemId, label: &str) -> Result<Vec<String>, EngineError> {
        let Some(index) = index_with_history(self)? else {
            let has_commits = self
                .repos()
                .all()
                .iter()
                .any(|one| matches!(one.git().head(), Ok(Some(_))));
            if has_commits {
                return Err(EngineError::Invalid(format!(
                    "cannot read the commit history, so stone cannot tell whether a commit names {label}; \
                     archive it instead with `stone task move {label} archived`"
                )));
            }
            return Ok(Vec::new());
        };
        let mut stmt = index.conn().prepare_cached(
            "SELECT DISTINCT c.sha FROM commit_refs r \
             JOIN commits c ON c.repo = r.repo AND c.sha = r.sha \
             WHERE r.kind = 'task' AND r.ref_id = ?1 ORDER BY c.ts DESC, c.sha ASC",
        )?;
        let rows = stmt.query_map([id.as_str()], |row| row.get::<_, String>(0))?;
        let mut shas = Vec::new();
        for sha in rows {
            let sha = sha?;
            shas.push(sha.get(..7).unwrap_or(&sha).to_string());
        }
        Ok(shas)
    }
}
