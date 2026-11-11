//! The CLI wiring over the real binary: every command answers on a small store, `serve` and `mcp` start,
//! and one table for wrong input. Engine behavior is checked in the engine's own tests.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

const STONE: &str = env!("CARGO_BIN_EXE_stone");

static NEXT: AtomicU32 = AtomicU32::new(0);

const UTIL: &str = "pub fn helper() -> u32 {\n    1\n}\n";

/// A temp folder to run `stone` in, removed on drop.
struct Store(PathBuf);

impl Store {
    fn new() -> Store {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("stone-cli-features-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let store = Store(dir);
        store.write("src/lib.rs", "pub mod util;\n");
        store.write("src/util.rs", UTIL);
        store
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.0.join(rel)).unwrap()
    }

    fn command(&self, program: &str, args: &[&str]) -> Command {
        let mut cmd = Command::new(program);
        cmd.args(args).current_dir(&self.0);
        cmd
    }

    /// The exit code and every line the command printed, on stdout and stderr.
    fn run(&self, args: &[&str]) -> (i32, String) {
        let out = self.command(STONE, args).output().unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.code().unwrap(), text)
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The arguments, the exit code, and one whole line the output must contain.
type Row<'a> = (&'a [&'a str], i32, &'a str);

fn expect(store: &Store, rows: &[Row]) {
    for (args, code, line) in rows {
        let (got, text) = store.run(args);
        assert_eq!(got, *code, "stone {}: {text}", args.join(" "));
        assert!(
            text.lines().any(|l| l == *line),
            "stone {}: no line `{line}` in:\n{text}",
            args.join(" ")
        );
    }
}

#[test]
fn every_command_answers_on_a_small_store() {
    let store = Store::new();
    expect(
        &store,
        &[(&["init", "--prefix", "CS"], 0, "stone/ is a git repo of its own")],
    );
    store.write("stone/config.yml", "project: t\ntask_prefix: CS\ncode_roots: [src/]\n");

    expect(
        &store,
        &[
            (&["check"], 0, "no problems"),
            (&["check", "--syntax"], 0, "no problems"),
            (&["new", "task", "--title", "First task"], 0, "CS-1"),
            (
                &[
                    "new",
                    "doc",
                    "--title",
                    "Guide",
                    "--section",
                    "Install",
                    "--text",
                    "Run the installer with sudo.",
                ],
                0,
                "docs/guide",
            ),
            (&["new", "container", "--title", "Api"], 0, "arch/api"),
            (
                &[
                    "new",
                    "component",
                    "--title",
                    "Util",
                    "--set",
                    "code=src/**",
                    "--link",
                    "arch/api",
                ],
                0,
                "arch/util",
            ),
            (&["list"], 0, "CS-1  task  todo  First task"),
            (&["get", "CS-1", "--json"], 0, "  \"short\": \"CS-1\","),
            (&["get", "CS-1"], 0, "title: First task"),
            (&["get", "docs/guide#install"], 0, "Run the installer with sudo."),
            (
                &["search", "installer"],
                0,
                "docs/guide#install  L9-11  doc  Run the installer with sudo.",
            ),
            (&["edit", "CS-1", "--set", "priority=high"], 0, "updated CS-1"),
            (
                &["edit", "CS-1", "--section", "notes", "--text", "n", "--replace-nested"],
                0,
                "updated CS-1",
            ),
            (
                &[
                    "edit",
                    "CS-1",
                    "--section",
                    "acceptance-criteria",
                    "--text",
                    "- [ ] a\n- [ ] b",
                ],
                0,
                "updated CS-1",
            ),
            (
                &["edit", "CS-1", "--check-ac", "1", "--check-ac", "2"],
                0,
                "updated CS-1",
            ),
            (&["link", "CS-1", "docs/guide"], 0, "updated CS-1"),
            (&["task", "next"], 0, "CS-1  high  First task"),
            (&["task", "check", "CS-1"], 1, "CS-1 is not ready:"),
            (&["task", "move", "CS-1", "in-progress", "--force"], 0, "updated CS-1"),
            (&["task", "fill-closed"], 0, "0 tasks filled"),
            (&["commit-msg"], 0, "Refs: CS-1"),
            (
                &["code", "find", "helper"],
                0,
                "  util::helper  L1-3  function  pub fn helper() -> u32",
            ),
            (
                &["code", "outline", "src/util.rs"],
                0,
                "  function helper() : u32  L1-3",
            ),
            (&["code", "uses", "helper"], 0, "next: git grep -n -F -e \"helper\""),
            (&["code", "deps", "util"], 0, "util depends on nothing"),
            (&["code", "impact", "helper"], 0, "nothing depends on util::helper"),
            (
                &["code", "path", "helper", "helper"],
                0,
                "util::helper to util::helper  forward",
            ),
            (&["code", "entries"], 0, "no entry points found"),
            (&["code", "explain", "arch/util"], 0, "src/util.rs  L1-3"),
            (&["arch", "hld"], 0, "HLD"),
            (&["arch", "lld", "arch/api"], 0, "LLD arch/api  Api"),
            (&["arch", "lld", "arch/api", "--budget", "400"], 0, "LLD arch/api  Api"),
            (&["scan"], 0, "0 written, 0 unchanged"),
            (
                &["arch", "map"],
                0,
                "arch/api  Api  no project is named like it; save its folder with --pick arch/api=<folder>",
            ),
            (&["map"], 0, "Components:"),
            (
                &["map", "--install-hook"],
                0,
                "added the session hook to .claude/settings.json",
            ),
            (
                &["map", "--install-hook"],
                0,
                "no change: .claude/settings.json already runs the map",
            ),
            (&["instructions"], 0, "# What stone holds"),
            (&["instructions", "plan"], 0, "# Planning in stone"),
            (
                &["rename", "docs/guide", "docs/manual"],
                0,
                "renamed stone/docs/guide.md -> stone/docs/manual.md",
            ),
            (
                &["renumber", "stone/tasks/CS-1.md", "--dry-run"],
                0,
                "would renumber stone/tasks/CS-1.md -> stone/tasks/CS-<next>.md",
            ),
            (&["new", "task", "--title", "Spike"], 0, "CS-2"),
            (&["new", "task", "--title", "Second"], 0, "CS-3"),
            (&["delete", "CS-2"], 0, "removed CS-2 (stone/tasks/CS-2.md)"),
            (&["task", "remove", "CS-3"], 0, "removed CS-3 (stone/tasks/CS-3.md)"),
        ],
    );

    // `--note` reaches the engine: the same text twice leaves one dated bullet under Notes.
    for _ in 0..2 {
        let (code, text) = store.run(&["edit", "CS-1", "--note", "x"]);
        assert_eq!(code, 0, "stone edit --note: {text}");
    }
    let task = store.read("stone/tasks/CS-1.md");
    let notes: Vec<&str> = task
        .lines()
        .filter(|l| l.starts_with("- ") && l.ends_with(": x") && l.len() == "- 2026-01-01: x".len())
        .collect();
    assert_eq!(notes.len(), 1, "one dated note line in:\n{task}");

    // The git commands read the store's own repo, which `init` made; they need the file staged.
    let staged = store.command("git", &["-C", "stone", "add", "-A"]).output().unwrap();
    assert!(staged.status.success());
    expect(
        &store,
        &[
            (&["why", "stone/tasks/CS-1.md"], 0, "no commits touch it yet"),
            (&["history", "CS-1"], 0, "CS-1 has no commits yet"),
            (&["timeline"], 0, "no events"),
            (&["precedents", "CS-1"], 0, "no finished task made a similar change"),
            (&["brief", "CS-1"], 0, "CS-1  First task  in-progress"),
        ],
    );

    // `mcp` answers the initialize request on stdin and ends when stdin closes.
    let mut mcp = store
        .command(STONE, &["mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"features","version":"0"}}}"#;
    writeln!(mcp.stdin.take().unwrap(), "{init}").unwrap();
    let out = mcp.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stdout).contains(r#""serverInfo":{"name":"stone""#),
        "mcp said: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    // `serve` prints where it listens; port 0 lets the system pick a free one.
    let mut serve = store
        .command(STONE, &["serve", "--port", "0", "--no-watch"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut first = String::new();
    BufReader::new(serve.stdout.take().unwrap())
        .read_line(&mut first)
        .unwrap();
    serve.kill().unwrap();
    serve.wait().unwrap();
    assert!(
        first.starts_with("stone serve: http://127.0.0.1:"),
        "serve said: {first}"
    );
}

#[test]
fn errors() {
    let store = Store::new();
    expect(
        &store,
        &[(&["init", "--prefix", "CS"], 0, "stone/ is a git repo of its own")],
    );
    store.write(
        "stone/tasks/CS-1.md",
        "---\ntype: task\ntitle: First\nstatus: todo\nlinks: [docs/guide]\n---\n",
    );
    store.write("stone/docs/guide.md", "---\ntype: doc\ntitle: Guide\n---\n\nIntro.\n");
    let before = store.read("stone/tasks/CS-1.md");

    // Clap refuses with exit 2 and an `error:` line; a refused command exits 1 with a `✗` line.
    expect(
        &store,
        &[
            (&["bogus"], 2, "error: unrecognized subcommand 'bogus'"),
            (
                &["get"],
                2,
                "error: the following required arguments were not provided:",
            ),
            (&["get", "CS-99"], 1, "✗ CS-99 not found. Did you mean CS-1?"),
            (
                &["edit", "CS-99", "--set", "status=done"],
                1,
                "✗ CS-99 not found. Did you mean CS-1?",
            ),
            (
                &["task", "move", "CS-1", "bogus"],
                1,
                "✗ status `bogus` is not valid for a task; use one of: draft, todo, in-progress, review, done, archived",
            ),
            (
                &["task", "move", "CS-1", "done"],
                1,
                "✗ CS-1 has no ## Outcome; add one with `stone edit CS-1 --section outcome`",
            ),
            (
                &["delete", "docs/guide"],
                1,
                "✗ docs/guide is linked from CS-1, so deleting it would leave broken links; change those links first, or delete it anyway with `stone delete docs/guide --force`",
            ),
            (
                &["instructions", "bogus"],
                1,
                "✗ unknown guide bogus; use overview, plan, do, finish, debug or design",
            ),
            (&["renumber", "nope.md"], 1, "✗ nope.md not found"),
            (&["rename", "docs/nope", "docs/x"], 1, "✗ docs/nope not found"),
            (&["history", "nope"], 1, "✗ nope not found"),
            (&["code", "explain", "arch/nope"], 1, "✗ arch/nope not found"),
            (
                &["brief", "docs/guide"],
                1,
                "✗ docs/guide is not a task; give a task id, for example CS-12",
            ),
            (
                &["why", "src/util.rs"],
                1,
                "✗ src/util.rs is not under git; run `git init` in its folder or list the folder in code_roots",
            ),
        ],
    );
    // No refused command touched the file.
    assert_eq!(store.read("stone/tasks/CS-1.md"), before);
}

#[test]
fn an_explicit_root_is_never_replaced_by_a_parent_store() {
    let store = Store::new();
    expect(
        &store,
        &[
            (&["init", "--prefix", "CS"], 0, "stone/ is a git repo of its own"),
            (&["new", "task", "--title", "First"], 0, "CS-1"),
        ],
    );
    let config_before = store.read("stone/config.yml");
    let tasks_before = store.run(&["list"]).1;
    let sub = store.0.join("src");
    let sub_arg = sub.to_str().unwrap();
    let refusal = format!("✗ {sub_arg} holds no store; run stone init there, or pass the repository root");

    for args in [
        vec!["--root", sub_arg, "config", "add-code-root", "src"],
        vec!["--root", sub_arg, "new", "task", "--title", "Stray"],
        vec!["--root", sub_arg, "get", "CS-1"],
    ] {
        let (code, text) = store.run(&args);
        assert_eq!(code, 1, "stone {}: {text}", args.join(" "));
        assert!(
            text.lines().any(|l| l == refusal),
            "stone {}: no line `{refusal}` in:\n{text}",
            args.join(" ")
        );
    }
    assert_eq!(store.read("stone/config.yml"), config_before);
    assert_eq!(store.run(&["list"]).1, tasks_before);

    // The store itself works as an explicit root, and with no --root a subfolder still finds it.
    let root_arg = store.0.to_str().unwrap();
    let (code, text) = store.run(&["--root", root_arg, "get", "CS-1"]);
    assert_eq!(code, 0, "{text}");
    let out = store
        .command(STONE, &["get", "CS-1"])
        .current_dir(&sub)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
}
