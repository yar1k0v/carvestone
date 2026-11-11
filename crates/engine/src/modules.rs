//! Modules: groups of files that call each other a lot, found by Louvain over
//! the file-to-file code edges. Clustering runs per project and first-level
//! folder under the project root, so a module never spans two of them. Code in separate
//! repos (`meta.code_projects`) is clustered apart too, whatever the projects are called.
//!
//! The `modules` table holds one row per member file: `label` is the module id
//! (`mod:<project>/<folder>/<k>`), `project` the project name. The readable label
//! (the longest common folder) is worked out from the members when read.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};

use carvestone_graph::louvain;
use rusqlite::{Connection, Transaction, TransactionBehavior, params};
use serde::Serialize;

use crate::code::open;
use crate::code_index::{contains, meta_value, set_meta, stored_project_dirs};
use crate::repo::{EngineError, Repo};

/// Weight of an exact edge between two files, and of an inferred one.
const EXACT: f64 = 1.0;
const INFERRED: f64 = 0.3;
/// Stands in for the project name of a file no project file governs.
const NO_PROJECT: &str = "-";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Module {
    /// `mod:<project>/<folder>/<k>`; `k` ranks the modules of one project and folder, biggest first.
    pub id: String,
    pub project: String,
    /// The longest folder all member files sit under.
    pub label: String,
    /// Member paths, sorted.
    pub files: Vec<String>,
}

struct Found {
    module: Module,
    file_ids: Vec<i64>,
}

struct FileRow {
    id: i64,
    path: String,
    project: String,
}

fn load_files(conn: &Connection) -> Result<Vec<FileRow>, EngineError> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.path, ifnull((SELECT s.project FROM symbols s \
                WHERE s.file_id = f.id AND s.kind <> 'project' ORDER BY s.id LIMIT 1), '') \
         FROM files f WHERE f.lang IS NOT NULL AND f.lang <> 'project' ORDER BY f.path",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(FileRow {
            id: row.get(0)?,
            path: row.get(1)?,
            project: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Project roots by project name.
fn load_roots(conn: &Connection) -> Result<HashMap<String, Vec<String>>, EngineError> {
    let mut stmt = conn.prepare("SELECT name, root FROM projects ORDER BY root")?;
    let mut roots: HashMap<String, Vec<String>> = HashMap::new();
    for row in stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
        let (name, root) = row?;
        roots.entry(name).or_default().push(root);
    }
    Ok(roots)
}

/// Edge weight between two files, from direct edges in both directions. An
/// edge the resolver could not pin to one target has no far end and counts for
/// nothing. Edges into name buckets are left to [`load_bucket_links`].
fn load_weights(conn: &Connection) -> Result<BTreeMap<(i64, i64), f64>, EngineError> {
    // `file_id` is already the source file, so only the target needs a symbol
    // lookup. Grouping in SQL gives one row per file pair.
    let mut stmt = conn.prepare(
        "SELECT min(e.file_id, d.file_id), max(e.file_id, d.file_id), \
         sum(e.exact = 1), sum(e.exact = 0) \
         FROM edges e JOIN symbols d ON d.id = e.dst_id \
         WHERE e.dst_id IS NOT NULL AND e.file_id <> d.file_id \
         GROUP BY min(e.file_id, d.file_id), max(e.file_id, d.file_id)",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, f64>(3)?,
        ))
    })?;
    let mut weights: BTreeMap<(i64, i64), f64> = BTreeMap::new();
    for row in rows {
        let (a, b, exact, inferred) = row?;
        *weights.entry((a, b)).or_insert(0.0) += exact * EXACT + inferred * INFERRED;
    }
    Ok(weights)
}

/// One name bucket as clustering sees it: how many sites each file points at
/// it with, and how many members each file declares.
#[derive(Default)]
struct BucketLinks {
    sites: Vec<(i64, f64)>,
    members: Vec<(i64, f64)>,
}

/// Every bucket's site and member files. Clustering gives a bucket its own
/// node instead of a pair per site and member, so a name shared by n files
/// costs n links, not n² (see [`compute`]).
fn load_bucket_links(conn: &Connection) -> Result<Vec<BucketLinks>, EngineError> {
    let mut links: BTreeMap<i64, BucketLinks> = BTreeMap::new();
    let mut stmt =
        conn.prepare("SELECT dst_bucket, file_id, count(*) FROM edges WHERE dst_bucket IS NOT NULL GROUP BY 1, 2")?;
    for row in stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, f64>(2)?))
    })? {
        let (bucket, file, count) = row?;
        links.entry(bucket).or_default().sites.push((file, count));
    }
    let mut stmt = conn.prepare(
        "SELECT m.bucket_id, s.file_id, count(*) FROM bucket_members m \
         JOIN symbols s ON s.id = m.symbol_id GROUP BY 1, 2",
    )?;
    for row in stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, f64>(2)?))
    })? {
        let (bucket, file, count) = row?;
        links.entry(bucket).or_default().members.push((file, count));
    }
    // Bucket ids follow insertion order, which differs between a pass and a
    // rebuild; node numbers must not, so order by what a bucket is.
    let mut keys: Vec<(String, String, String, i64)> = Vec::new();
    let mut stmt = conn.prepare("SELECT lang, scope, name, id FROM buckets")?;
    for row in stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))? {
        keys.push(row?);
    }
    keys.sort();
    Ok(keys.into_iter().filter_map(|(_, _, _, id)| links.remove(&id)).collect())
}

/// The first folder of `path` under the project `root`; `.` for a file directly in it.
fn folder_of(path: &str, root: &str) -> String {
    let rel = if root.is_empty() {
        path
    } else {
        path.strip_prefix(root)
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or(path)
    };
    rel.split_once('/').map_or(".", |(first, _)| first).to_string()
}

/// The longest folder all `paths` sit under; `.` when they share none.
pub(crate) fn common_folder(paths: &[String]) -> String {
    let mut common: Option<Vec<&str>> = None;
    for path in paths {
        let dir: Vec<&str> = path
            .rsplit_once('/')
            .map_or(Vec::new(), |(dir, _)| dir.split('/').collect());
        common = Some(match common {
            None => dir,
            Some(prev) => prev
                .iter()
                .zip(&dir)
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| *a)
                .collect(),
        });
    }
    match common {
        Some(parts) if !parts.is_empty() => parts.join("/"),
        _ => ".".to_string(),
    }
}

/// Order modules by project and folder, then rank; the id's last segment is the rank.
fn order_key(id: &str) -> (String, u64) {
    let (prefix, rank) = id.rsplit_once('/').unwrap_or((id, "0"));
    (prefix.to_string(), rank.parse::<u64>().unwrap_or(0))
}

/// Clusters the current edges from scratch.
fn compute(conn: &Connection) -> Result<Vec<Found>, EngineError> {
    let files = load_files(conn)?;
    let roots = load_roots(conn)?;
    let weights = load_weights(conn)?;
    let project_dirs = stored_project_dirs(conn)?;

    // Files in path order, grouped by (project, folder).
    let mut groups: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (index, file) in files.iter().enumerate() {
        let root = roots
            .get(&file.project)
            .and_then(|list| {
                list.iter()
                    .filter(|root| root.is_empty() || file.path.starts_with(&format!("{root}/")))
                    .max_by_key(|root| root.len())
            })
            .map_or("", String::as_str);
        let project = if file.project.is_empty() {
            NO_PROJECT.to_string()
        } else {
            file.project.clone()
        };
        // Two projects of one name in different repos stay apart: the repo folder joins the key.
        let folder = match project_dirs
            .iter()
            .filter(|dir| contains(dir, &file.path))
            .max_by_key(|dir| dir.len())
        {
            Some(dir) => format!("{dir}/{}", folder_of(&file.path, root)),
            None => folder_of(&file.path, root),
        };
        groups.entry((project, folder)).or_default().push(index);
    }
    let mut place: HashMap<i64, (usize, usize)> = HashMap::new();
    let mut edges: Vec<Vec<(u32, u32, f64)>> = vec![Vec::new(); groups.len()];
    for (g, members) in groups.values().enumerate() {
        for (local, &index) in members.iter().enumerate() {
            place.insert(files[index].id, (g, local));
        }
    }
    for (&(a, b), &weight) in &weights {
        if let (Some(&(ga, la)), Some(&(gb, lb))) = (place.get(&a), place.get(&b))
            && ga == gb
        {
            edges[ga].push((la as u32, lb as u32, weight));
        }
    }
    // A bucket becomes a virtual node in each group that holds both a site and
    // a member of it. Each site of k members spreads 0.3 over them, as a pair
    // per member would: a site file links with 0.3 * sites * k_g / k and a
    // member file with 0.3 * s_g * members / k, where k_g and s_g are the
    // group's share of members and sites. Both sides then carry the weight the
    // pairs between them would have.
    let mut nodes: Vec<u32> = groups.values().map(|members| members.len() as u32).collect();
    for links in &load_bucket_links(conn)? {
        let k: f64 = links.members.iter().map(|&(_, n)| n).sum();
        type Side = Vec<(u32, f64)>;
        let mut per_group: BTreeMap<usize, (Side, Side)> = BTreeMap::new();
        for &(file, n) in &links.sites {
            if let Some(&(g, local)) = place.get(&file) {
                per_group.entry(g).or_default().0.push((local as u32, n));
            }
        }
        for &(file, n) in &links.members {
            if let Some(&(g, local)) = place.get(&file) {
                per_group.entry(g).or_default().1.push((local as u32, n));
            }
        }
        for (g, (sites, members)) in per_group {
            if sites.is_empty() || members.is_empty() {
                continue;
            }
            let s_g: f64 = sites.iter().map(|&(_, n)| n).sum();
            let k_g: f64 = members.iter().map(|&(_, n)| n).sum();
            let node = nodes[g];
            nodes[g] += 1;
            for (local, n) in sites {
                edges[g].push((local, node, INFERRED * n * k_g / k));
            }
            for (local, n) in members {
                edges[g].push((node, local, INFERRED * s_g * n / k));
            }
        }
    }

    let mut found = Vec::new();
    for (g, ((project, folder), members)) in groups.into_iter().enumerate() {
        let community = louvain(nodes[g] as usize, &edges[g]);
        let mut buckets: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        // Bucket nodes come after the files and are no module's members.
        for (local, c) in community.into_iter().take(members.len()).enumerate() {
            buckets.entry(c).or_default().push(local);
        }
        // Locals ascend with the path, so the first local is the smallest path.
        let mut ranked: Vec<Vec<usize>> = buckets.into_values().collect();
        ranked.sort_by_key(|bucket| (Reverse(bucket.len()), bucket[0]));
        for (k, bucket) in ranked.into_iter().enumerate() {
            let paths: Vec<String> = bucket.iter().map(|&l| files[members[l]].path.clone()).collect();
            found.push(Found {
                module: Module {
                    id: format!("mod:{project}/{folder}/{}", k + 1),
                    project: project.clone(),
                    label: common_folder(&paths),
                    files: paths,
                },
                file_ids: bucket.iter().map(|&l| files[members[l]].id).collect(),
            });
        }
    }
    found.sort_by_cached_key(|f| order_key(&f.module.id));
    Ok(found)
}

/// The stored modules, in id order.
pub(crate) fn stored(conn: &Connection) -> Result<Vec<Module>, EngineError> {
    let mut stmt = conn.prepare(
        "SELECT m.label, m.project, f.path FROM modules m JOIN files f ON f.id = m.file_id \
         ORDER BY m.label, f.path",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut by_id: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    for row in rows {
        let (id, project, path) = row?;
        by_id.entry(id).or_insert_with(|| (project, Vec::new())).1.push(path);
    }
    let mut modules: Vec<Module> = by_id
        .into_iter()
        .map(|(id, (project, files))| Module {
            label: common_folder(&files),
            id,
            project,
            files,
        })
        .collect();
    modules.sort_by_cached_key(|m| order_key(&m.id));
    Ok(modules)
}

/// Brings the stored modules in line with the edges when `graph_version` moved
/// past `modules_version`. A pass still filling the graph is left alone.
/// Returns whether anything was recomputed.
pub(crate) fn recompute_if_stale(conn: &Connection) -> Result<bool, EngineError> {
    let behind = |conn: &Connection| -> Result<bool, EngineError> {
        if meta_value(conn, "code_building")?.as_deref() == Some("1") {
            return Ok(false);
        }
        Ok(meta_value(conn, "graph_version")? != meta_value(conn, "modules_version")?)
    };
    if !behind(conn)? {
        return Ok(false);
    }
    // Lock first, then look again: another process may have recomputed already.
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    if !behind(&tx)? {
        return Ok(false);
    }
    let found = compute(&tx)?;
    let computed: Vec<Module> = found.iter().map(|f| f.module.clone()).collect();
    if stored(&tx)? != computed {
        tx.execute("DELETE FROM modules", [])?;
        let mut insert = tx.prepare_cached("INSERT INTO modules(project, label, file_id) VALUES (?1, ?2, ?3)")?;
        for f in &found {
            for file_id in &f.file_ids {
                insert.execute(params![f.module.project, f.module.id, file_id])?;
            }
        }
    }
    let version = meta_value(&tx, "graph_version")?.unwrap_or_else(|| "0".to_string());
    set_meta(&tx, "modules_version", &version)?;
    tx.commit()?;
    Ok(true)
}

/// The modules of the repository, in id order, with the index brought up to date first.
pub fn modules(repo: &Repo) -> Result<Vec<Module>, EngineError> {
    let index = open(repo)?;
    stored(index.conn())
}

/// Clusters the current code edges from scratch and returns the result without
/// storing it. After a refresh it equals `modules`.
pub fn compute_modules(repo: &Repo) -> Result<Vec<Module>, EngineError> {
    let index = open(repo)?;
    Ok(compute(index.conn())?.into_iter().map(|f| f.module).collect())
}
