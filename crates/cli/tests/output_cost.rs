//! What the common read calls cost an agent, measured on the real binary over a
//! fixed repo. Tokens are chars / 4 of stdout, the measure `Hit::cost` uses.
//! Each call has a bar about 20% above its measured value, so output that grows
//! fails here. Run with `-- --nocapture` to read the numbers.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

const STONE: &str = env!("CARGO_BIN_EXE_stone");

static NEXT: AtomicU32 = AtomicU32::new(0);

struct Repo(PathBuf);

impl Repo {
    fn new() -> Repo {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("stone-output-cost-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let repo = Repo(dir);
        assert_eq!(repo.run(&["init", "--prefix", "CS"]).0, 0);
        repo.write(
            "stone/config.yml",
            "project: shop\ntask_prefix: CS\ncode_roots: [src/]\n",
        );
        repo.tasks();
        repo.docs();
        repo.code();
        repo
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// Eight tasks, a mix of states, each with a description and criteria.
    fn tasks(&self) {
        for n in 1..=8 {
            let status = if n <= 3 { "done" } else { "todo" };
            let outcome = if status == "done" {
                "\n## Outcome\n\nThe ledger now reconciles each day and the nightly report matches the bank file.\n"
            } else {
                ""
            };
            self.write(
                &format!("stone/tasks/CS-{n}.md"),
                &format!(
                    "---\ntype: task\ntitle: Reconcile ledger step {n}\nstatus: {status}\npriority: medium\ncreated: 2026-01-0{n}\n---\n\n\
                     ## Description\n\nThe ledger must reconcile every posting against the bank file. Step {n} \
                     covers the batch import, the matching rules and the report that finance reads each morning. \
                     Unmatched postings wait in a review queue until someone clears them.\n\n\
                     ## Acceptance criteria\n\n- [ ] postings match the bank file\n- [ ] unmatched postings are queued\n\
                     - [ ] the morning report lists every queue\n{outcome}"
                ),
            );
        }
    }

    /// Six docs with four sections each, all talking about the ledger.
    fn docs(&self) {
        for n in 1..=6 {
            let mut text = format!(
                "---\ntype: doc\ntitle: Ledger operations part {n}\nstatus: stable\n---\n\n# Ledger operations part {n}\n"
            );
            for section in ["Overview", "Matching rules", "Review queue", "Reports"] {
                text.push_str(&format!(
                    "\n## {section}\n\nThe ledger reconcile run reads the bank file and posts each line. In {section} \
                     of part {n} the reconcile step keeps a count of matched and unmatched postings and writes it \
                     to the report so that finance can compare the ledger with the bank balance.\n"
                ));
            }
            self.write(&format!("stone/docs/ledger-{n}.md"), &text);
        }
    }

    /// A component plus eight handlers that all post to one ledger and call the next handler.
    fn code(&self) {
        self.write(
            "stone/arch/shop.md",
            "---\ntype: component\ntitle: Shop\nstatus: stable\ncode: ['src/**']\n---\n\n## Responsibilities\n\nPosts orders to the ledger.\n",
        );
        self.write(
            "src/Shop/Shop.csproj",
            "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><AssemblyName>Shop</AssemblyName></PropertyGroup></Project>",
        );
        self.write(
            "src/Shop/Ledger.cs",
            "namespace Shop {\n public class Ledger {\n  public void Post() { }\n }\n}\n",
        );
        for n in 0..8 {
            let next = if n < 7 {
                format!(" new Handler{}().Run();", n + 1)
            } else {
                String::new()
            };
            self.write(
                &format!("src/Shop/Handler{n}.cs"),
                &format!(
                    "namespace Shop {{\n public class Handler{n} {{\n  public void Run() {{\n   var ledger = new Ledger();\n   ledger.Post();{next}\n  }}\n }}\n}}\n"
                ),
            );
        }
    }

    /// Exit code and stdout.
    fn run(&self, args: &[&str]) -> (i32, String) {
        let out = Command::new(STONE)
            .current_dir(&self.0)
            .args(args)
            .env("STONE_AGENT", "claude-code")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
    }

    /// Everything one call prints, in tokens (chars / 4, as `Hit::cost` counts); the number goes to the test output.
    fn cost(&self, label: &str, args: &[&str]) -> usize {
        let (code, out) = self.run(args);
        assert_eq!(code, 0, "{label}: {out}");
        let tokens = out.chars().count().div_ceil(4);
        println!("output_cost {label}: {tokens} tokens");
        tokens
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// One typical call and the most tokens it may print.
struct Call {
    label: &'static str,
    args: &'static [&'static str],
    bar: usize,
}

#[test]
fn typical_calls_stay_under_their_bars() {
    // Measured 140, 62, 72, 80, 80, 154, 170 and 270 tokens; each bar is that plus 20%.
    let calls = [
        Call {
            label: "get CS-1",
            args: &["get", "CS-1"],
            bar: 168,
        },
        Call {
            label: "get CS-1#description",
            args: &["get", "CS-1#description"],
            bar: 75,
        },
        Call {
            label: "list --type task",
            args: &["list", "--type", "task"],
            bar: 87,
        },
        Call {
            label: "search reconcile ledger",
            args: &["search", "reconcile ledger"],
            bar: 96,
        },
        Call {
            label: "search reconcile ledger --budget 200",
            args: &["search", "reconcile ledger", "--budget", "200"],
            bar: 96,
        },
        Call {
            label: "code uses Shop.Ledger.Post",
            args: &["code", "uses", "Shop.Ledger.Post"],
            bar: 185,
        },
        Call {
            label: "code impact Shop.Ledger.Post",
            args: &["code", "impact", "Shop.Ledger.Post"],
            bar: 204,
        },
        Call {
            label: "code explain arch/shop",
            args: &["code", "explain", "arch/shop"],
            bar: 324,
        },
    ];
    let repo = Repo::new();
    let mut over = Vec::new();
    for call in &calls {
        let tokens = repo.cost(call.label, call.args);
        if tokens > call.bar {
            over.push(format!("{}: {tokens} tokens, bar {}", call.label, call.bar));
        }
    }
    assert!(over.is_empty(), "calls over their bar:\n{}", over.join("\n"));
}

#[test]
fn budget_output_stays_within_the_budget() {
    let repo = Repo::new();
    let calls: [&[&str]; 11] = [
        &["search", "reconcile ledger"],
        &["code", "uses", "Shop.Ledger.Post"],
        &["code", "impact", "Shop.Ledger.Post"],
        &["code", "explain", "arch/shop"],
        &["get", "CS-1"],
        &["get", "docs/ledger-1"],
        &["list"],
        &["arch", "hld"],
        &["instructions"],
        &["history", "CS-1"],
        &["timeline"],
    ];
    for budget in [50, 100, 200, 400] {
        let b = budget.to_string();
        for call in calls {
            let args: Vec<&str> = call.iter().copied().chain(["--budget", b.as_str()]).collect();
            let label = format!("{} --budget {budget}", call.join(" "));
            // Everything printed counts, the cut-off sentence included.
            let tokens = repo.cost(&label, &args);
            assert!(tokens <= budget, "{label} printed {tokens} tokens");
        }
    }
    let (code, out) = repo.run(&["get", "CS-1", "--budget", "50"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.lines().last().unwrap_or_default().ends_with("; raise --budget"),
        "no cut-off line in:\n{out}"
    );
}
