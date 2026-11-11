//! The project at the start of a session: the components with their code and entry
//! points, the tasks in review, the next ready tasks, how far each milestone is and
//! the state of the working tree.

use serde::Serialize;

use crate::arch_graph::arch_graph;
use crate::code::CodeQuery;
use crate::code_entries;
use crate::query::{ListFilter, ListRow};
use crate::repo::{EngineError, Repo};

/// Ready tasks the map lists; the rest are counted only.
pub const READY: usize = 5;

/// Everything `stone map` prints, in the order a session asks for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectMap {
    pub project: String,
    pub git: Option<GitState>,
    pub components: Vec<MapComponent>,
    pub review: Vec<MapTask>,
    pub ready: Vec<MapTask>,
    pub ready_total: usize,
    pub milestones: Vec<MapMilestone>,
}

/// The working tree of the first git repo at or below the root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitState {
    pub branch: Option<String>,
    pub changed: usize,
}

/// An architecture element that names code, with the entry points the code graph found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MapComponent {
    pub id: String,
    pub title: String,
    pub code: Vec<String>,
    pub entries: Vec<String>,
}

/// A task as a session reads it: its id and its title.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MapTask {
    pub id: String,
    pub title: String,
}

/// A milestone, or any other item that tasks point at, with its task counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MapMilestone {
    pub id: String,
    pub title: String,
    pub done: usize,
    pub total: usize,
}

impl Repo {
    /// The map of the repository, in one pass over the store and the code index.
    pub fn project_map(&self) -> Result<ProjectMap, EngineError> {
        let found = code_entries::entries(
            self,
            &CodeQuery {
                target: String::new(),
                limit: usize::MAX,
                ..CodeQuery::default()
            },
        )?;
        let components = arch_graph(self)?
            .nodes
            .iter()
            .filter(|node| matches!(node.type_name.as_str(), "container" | "component"))
            .filter(|node| !node.code.is_empty())
            .map(|node| {
                // A group is labelled with the element's short id, or its id when it has none.
                let group = node
                    .short
                    .as_ref()
                    .into_iter()
                    .chain(std::iter::once(&node.id))
                    .find_map(|key| found.groups.iter().find(|group| group.component == *key));
                let entries = group
                    .map(|group| {
                        group
                            .entries
                            .iter()
                            .filter(|row| row.kind != "gone")
                            .map(|row| row.target.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                MapComponent {
                    id: node.id.clone(),
                    title: node.title.clone().unwrap_or_default(),
                    code: node.code.clone(),
                    entries,
                }
            })
            .collect();

        let review = self
            .list(&ListFilter {
                item_type: Some("task".into()),
                status: Some("review".into()),
                ..ListFilter::default()
            })?
            .into_iter()
            .map(map_task)
            .collect();

        let all_ready = self.next(usize::MAX)?;
        let ready_total = all_ready.len();
        let ready = all_ready.into_iter().take(READY).map(map_task).collect();

        let mut milestones = Vec::new();
        for row in self.list(&ListFilter {
            item_type: Some("milestone".into()),
            ..ListFilter::default()
        })? {
            if row.status.as_deref() == Some("done") {
                continue;
            }
            let id = row.short.clone().unwrap_or_else(|| row.id.clone());
            let tasks = self.list(&ListFilter {
                item_type: Some("task".into()),
                milestone: Some(id.clone()),
                ..ListFilter::default()
            })?;
            // An archived task is out of the count, as on the planning page.
            let live = |task: &&ListRow| task.status.as_deref() != Some("archived");
            let total = tasks.iter().filter(live).count();
            let done = tasks
                .iter()
                .filter(|task| task.status.as_deref() == Some("done"))
                .count();
            milestones.push(MapMilestone {
                id,
                title: row.title.clone().unwrap_or_default(),
                done,
                total,
            });
        }

        let git = self.repos().all().first().and_then(|found| {
            // A repo git cannot answer for is left out rather than failing the whole map.
            let changed = found.git().working_changes(&[]).ok()?.len();
            Some(GitState {
                branch: found.git().branch().ok().flatten(),
                changed,
            })
        });

        Ok(ProjectMap {
            project: self.store.config.project.clone(),
            git,
            components,
            review,
            ready,
            ready_total,
            milestones,
        })
    }
}

/// A row as a session reads it: its short id when it has one, and its title.
fn map_task(row: ListRow) -> MapTask {
    MapTask {
        id: row.short.unwrap_or(row.id),
        title: row.title.unwrap_or_default(),
    }
}
