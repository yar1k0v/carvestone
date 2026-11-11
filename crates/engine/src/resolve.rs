//! Turning what a person types into an entry: ids, anchors, did-you-mean.

use carvestone_core::{IdScheme, Section, sections};
use carvestone_store::Entry;

use crate::repo::EngineError;

/// The store folders an item id can start with.
const FOLDERS: [&str; 7] = ["docs", "plans", "tasks", "milestones", "decisions", "refs", "arch"];

#[derive(Debug)]
pub struct Snapshot {
    pub entries: Vec<Entry>,
    pub scheme: IdScheme,
}

impl Snapshot {
    pub fn resolve(&self, input: &str) -> Result<(&Entry, Option<String>), EngineError> {
        let id_part = input.split_once('#').map_or(input, |(id, _)| id);
        let parsed = self.scheme.parse(input).map_err(|_| self.not_found(id_part))?;
        if let Some(entry) = self.entries.iter().find(|e| e.id == parsed.id) {
            return Ok((entry, parsed.anchor));
        }
        let want = self.scheme.number(&parsed.id);
        let loose: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|e| {
                (want.is_some() && self.scheme.number(&e.id) == want)
                    || e.id.as_str().eq_ignore_ascii_case(parsed.id.as_str())
            })
            .collect();
        match loose.as_slice() {
            [] => Err(self.not_found(id_part)),
            [one] => Ok((one, parsed.anchor)),
            many => Err(EngineError::Ambiguous {
                input: id_part.to_string(),
                paths: many.iter().map(|e| e.path.clone()).collect(),
            }),
        }
    }

    pub fn find_section(&self, entry: &Entry, anchor: &str) -> Result<Section, EngineError> {
        let found = entry
            .item
            .as_ref()
            .map(|item| sections(&item.body, item.body_line))
            .unwrap_or_default();
        let wanted = anchor.to_lowercase();
        match found.iter().find(|s| s.anchor == wanted) {
            Some(s) => Ok(s.clone()),
            None => Err(EngineError::NoSection {
                id: self.label(entry),
                anchor: anchor.to_string(),
                anchors: found.into_iter().map(|s| s.anchor).collect(),
            }),
        }
    }

    pub fn label(&self, entry: &Entry) -> String {
        self.scheme
            .short(&entry.id)
            .unwrap_or_else(|| entry.id.as_str().to_string())
    }

    /// The closest existing label within two edits, for "did you mean".
    pub fn suggest(&self, input: &str) -> Option<String> {
        let id_part = input.split_once('#').map_or(input, |(id, _)| id);
        let query = match self.scheme.parse(id_part) {
            Ok(r) => self.scheme.link_text(&r),
            Err(_) => id_part.trim().to_string(),
        }
        .to_lowercase();
        let mut best: Option<(usize, String)> = None;
        for entry in &self.entries {
            let label = self.label(entry);
            let d = edit_distance(&query, &label.to_lowercase());
            if (1..=2).contains(&d) && best.as_ref().is_none_or(|(b, _)| d < *b) {
                best = Some((d, label));
            }
        }
        best.map(|(_, label)| label)
    }

    /// What to run instead when `input` is a file path or a folder-prefixed name (`plans/VC-3`)
    /// that names no item: `stone get <id>` when an item has that path or id, else to read the file
    /// directly when `on_disk` says it exists. `None` when neither applies.
    pub fn redirect(&self, input: &str, on_disk: bool) -> Option<String> {
        let (name, anchor) = match input.split_once('#') {
            Some((name, anchor)) => (name, Some(anchor)),
            None => (input, None),
        };
        let path = name.trim().replace('\\', "/");
        let path = path.trim_start_matches("./").trim_start_matches('/');
        let at = anchor.map(|a| format!("#{a}")).unwrap_or_default();
        let run = |entry: &Entry| format!("`stone get {}{at}`", self.label(entry));
        if let Some(entry) = self.entries.iter().find(|e| e.path == path) {
            return Some(format!(
                "`{name}` is a file path, not an item id; run {} instead",
                run(entry)
            ));
        }
        let (folder, rest) = path.split_once('/').unwrap_or(("", path));
        if FOLDERS.contains(&folder) {
            let bare = rest.strip_suffix(".md").unwrap_or(rest);
            if let Ok((entry, _)) = self.resolve(bare) {
                return Some(format!(
                    "`{name}` is not an item id, but {} exists; run {} instead",
                    self.label(entry),
                    run(entry)
                ));
            }
        }
        // A name that is no file either keeps the plain "not found".
        on_disk.then(|| format!("`{name}` is a file, not an item id; read the file directly instead"))
    }

    fn not_found(&self, id: &str) -> EngineError {
        EngineError::NotFound {
            id: id.to_string(),
            suggestion: self.suggest(id),
        }
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != *cb);
            cur.push(sub.min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}
