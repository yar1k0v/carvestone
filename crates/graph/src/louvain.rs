//! Louvain community detection over a small weighted graph. Deterministic: nodes
//! are visited in index order, ties keep the node where it is, and no random
//! source is used, so the same input always gives the same communities.

use std::collections::BTreeMap;

/// A gain must beat the current one by more than this to move a node.
const EPS: f64 = 1e-12;
/// Sweeps per level; a level normally settles in a handful.
const MAX_SWEEPS: usize = 100;

/// One level of the algorithm: neighbours per node (sorted, no self loops) and
/// the weight of each node's loop. A loop counts twice in the node's degree.
struct Level {
    adj: Vec<Vec<(usize, f64)>>,
    loops: Vec<f64>,
}

impl Level {
    fn from_pairs(n: usize, pairs: BTreeMap<(usize, usize), f64>, mut loops: Vec<f64>) -> Level {
        let mut adj = vec![Vec::new(); n];
        for ((a, b), w) in pairs {
            if a == b {
                loops[a] += w;
            } else {
                adj[a].push((b, w));
                adj[b].push((a, w));
            }
        }
        for list in &mut adj {
            list.sort_by_key(|&(node, _)| node);
        }
        Level { adj, loops }
    }

    fn degree(&self, node: usize) -> f64 {
        self.adj[node].iter().map(|&(_, w)| w).sum::<f64>() + 2.0 * self.loops[node]
    }

    /// Moves nodes between communities while modularity improves. Returns each
    /// node's community id and whether any node moved.
    fn local_moves(&self) -> (Vec<usize>, bool) {
        let n = self.adj.len();
        let degrees: Vec<f64> = (0..n).map(|i| self.degree(i)).collect();
        let total: f64 = degrees.iter().sum();
        let mut comm: Vec<usize> = (0..n).collect();
        if total <= 0.0 {
            return (comm, false);
        }
        let mut sums = degrees.clone();
        let mut any = false;
        for _ in 0..MAX_SWEEPS {
            let mut moved = false;
            for i in 0..n {
                let old = comm[i];
                let mut towards: BTreeMap<usize, f64> = BTreeMap::new();
                for &(j, w) in &self.adj[i] {
                    *towards.entry(comm[j]).or_insert(0.0) += w;
                }
                sums[old] -= degrees[i];
                let gain = |c: usize, w: f64| w - sums[c] * degrees[i] / total;
                let mut best = old;
                let mut best_gain = gain(old, towards.get(&old).copied().unwrap_or(0.0));
                for (&c, &w) in &towards {
                    let g = gain(c, w);
                    if c != old && g > best_gain + EPS {
                        best = c;
                        best_gain = g;
                    }
                }
                sums[best] += degrees[i];
                if best != old {
                    comm[i] = best;
                    moved = true;
                    any = true;
                }
            }
            if !moved {
                break;
            }
        }
        (comm, any)
    }

    /// The graph whose nodes are `comm`'s communities (dense ids `0..count`).
    fn collapse(&self, comm: &[usize], count: usize) -> Level {
        let mut loops = vec![0.0; count];
        let mut pairs: BTreeMap<(usize, usize), f64> = BTreeMap::new();
        for (i, list) in self.adj.iter().enumerate() {
            loops[comm[i]] += self.loops[i];
            for &(j, w) in list.iter().filter(|&&(j, _)| j > i) {
                let (a, b) = (comm[i].min(comm[j]), comm[i].max(comm[j]));
                *pairs.entry((a, b)).or_insert(0.0) += w;
            }
        }
        Level::from_pairs(count, pairs, loops)
    }
}

/// Ids `0..k` in order of first appearance, and `k`.
fn dense(ids: &[usize]) -> (Vec<usize>, usize) {
    let mut seen: BTreeMap<usize, usize> = BTreeMap::new();
    let mapped = ids
        .iter()
        .map(|&id| {
            let next = seen.len();
            *seen.entry(id).or_insert(next)
        })
        .collect();
    (mapped, seen.len())
}

/// Communities of `n` nodes over undirected weighted `edges` `(a, b, weight)`,
/// resolution 1.0. The result has one id per node; ids count up in node order,
/// so node 0 is always in community 0. Repeated pairs add up, edges with an end
/// outside `0..n` or a weight that is not positive are ignored.
pub fn louvain(n: usize, edges: &[(u32, u32, f64)]) -> Vec<u32> {
    let mut pairs: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    for &(a, b, w) in edges {
        let (a, b) = (a as usize, b as usize);
        if a < n && b < n && w.is_finite() && w > 0.0 {
            *pairs.entry((a.min(b), a.max(b))).or_insert(0.0) += w;
        }
    }
    let mut level = Level::from_pairs(n, pairs, vec![0.0; n]);
    let mut membership: Vec<usize> = (0..n).collect();
    loop {
        let (comm, moved) = level.local_moves();
        if !moved {
            break;
        }
        let (comm, count) = dense(&comm);
        for node in &mut membership {
            *node = comm[*node];
        }
        level = level.collapse(&comm, count);
    }
    let (membership, _) = dense(&membership);
    membership.into_iter().map(|c| c as u32).collect()
}
