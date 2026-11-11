//! Order for inferred edges. A name-only match is likelier to be the real
//! target when both ends are test code or both are not, and likelier still the
//! closer their paths sit. Nothing is dropped and nothing becomes exact: only
//! the order changes, so a limit or a budget cuts the least likely first.

// Whether a path is test code is the graph's call too, so there is one rule.
pub(crate) use carvestone_graph::is_test_path;

/// Whether a path is code that is not part of the architecture: tests,
/// benchmarks, or code a tool writes (EF migrations, designer files, `*.g.cs`).
pub(crate) fn is_support_path(path: &str) -> bool {
    if is_test_path(path) {
        return true;
    }
    let lower = path.to_ascii_lowercase();
    let mut segments: Vec<&str> = lower.split('/').collect();
    let file = segments.pop().unwrap_or("");
    segments.iter().any(|dir| {
        matches!(*dir, "migrations" | "benchmarks" | "benchmark" | "generated")
            || dir.ends_with(".benchmarks")
            || dir.ends_with(".benchmark")
    }) || file.ends_with(".designer.cs")
        || file.ends_with(".g.cs")
        || file.ends_with(".g.i.cs")
        || file.contains(".generated.")
        || file.ends_with("modelsnapshot.cs")
}

/// The number of leading directories two paths share.
pub(crate) fn proximity(a: &str, b: &str) -> usize {
    let dirs = |path: &'_ str| -> Vec<String> {
        let mut parts: Vec<String> = path.split('/').map(str::to_string).collect();
        parts.pop();
        parts
    };
    dirs(a).iter().zip(dirs(b).iter()).take_while(|(x, y)| x == y).count()
}

/// How likely a name-only match between code at `a` and code at `b` is real;
/// larger sorts first.
pub(crate) fn rank(a: &str, b: &str) -> (bool, usize) {
    (is_test_path(a) == is_test_path(b), proximity(a, b))
}
