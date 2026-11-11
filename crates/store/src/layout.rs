//! Where each item kind lives on disk.

use std::path::{Path, PathBuf};

use carvestone_core::ItemId;

#[derive(Debug, Clone)]
pub struct Layout {
    pub root: PathBuf,
    pub data_dir: PathBuf,
}

impl Layout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            data_dir: root.join("stone"),
            root,
        }
    }

    /// Walk up from `start` to the first directory containing `stone/config.yml`.
    pub fn discover(start: &Path) -> Option<Self> {
        start
            .ancestors()
            .find(|d| d.join("stone").join("config.yml").is_file())
            .map(Self::new)
    }

    pub fn index_path(&self) -> PathBuf {
        self.root.join(".stone").join("index.db")
    }

    /// File for an item id; the suffix is appended, not set, so `docs/v1.2` keeps its `.2`.
    pub fn item_path(&self, id: &ItemId) -> PathBuf {
        let mut path = self.data_dir.clone();
        let mut segments = id.as_str().split('/').peekable();
        while let Some(seg) = segments.next() {
            if segments.peek().is_some() {
                path.push(seg);
            } else {
                path.push(format!("{seg}.md"));
            }
        }
        path
    }

    pub fn lock_path(&self) -> PathBuf {
        self.root.join(".stone").join("lock")
    }
}
