//! `stone`: the primary interface for agents and humans.
//!
//! The commands mirror the PRD. Errors go to stderr as one line starting with
//! a cross mark; problems and results go to stdout.

mod lean;

use std::collections::{HashMap, HashSet};
use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use lean::Detail;

use carvestone_engine::agent_block::{self, AgentBlock};
use carvestone_engine::arch_c4 as c4;
use carvestone_engine::changed_files::changed_files_block;
use carvestone_engine::code::{self, CodeQuery, CodeResult, Outline};
use carvestone_engine::code_entries::{self, Entries};
use carvestone_engine::code_graph::{self, Explain, Impact, PathResult};
use carvestone_engine::code_native::{self, IndexerRun, LangSetup, NativeQuery, NativeReport, NativeStep};
use carvestone_engine::commit_msg::{self, HookState, UninstallState};
use carvestone_engine::create::{Initialised, NewItem, StoneRepo, Written, init_with};
use carvestone_engine::design;
use carvestone_engine::edit::Edit;
use carvestone_engine::fields::SetValue;
use carvestone_engine::guide::{Topic, guide_text, unknown_topic};
use carvestone_engine::history::{self, HistoryOpts};
use carvestone_engine::precedents::{self, Lookup, Precedents};
use carvestone_engine::query::{Backlink, GetResult, ListFilter, ListRow, libraries_note};
use carvestone_engine::remove::Removed;
use carvestone_engine::rename::Renamed;
use carvestone_engine::renumber::Renumbered;
use carvestone_engine::scan::{ScanOpts, ScanReport, ScanState, scan};
use carvestone_engine::search::{SearchQuery, SearchResult, search};
use carvestone_engine::service_map::{LibraryCode, ServiceCode};
use carvestone_engine::timeline::{self, Date, Event, EventKind, TimelineOpts, parse_date};
use carvestone_engine::why::{self, Why};
use carvestone_engine::{EngineError, Problem, Repo, Severity};
use carvestone_server::ServeOpts;
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "stone",
    version = concat!(env!("CARGO_PKG_VERSION"), env!("STONE_BUILD")),
    about = "Planning and knowledge store for humans and AI agents"
)]
struct Cli {
    /// Repository root (default: discovered from the current directory).
    #[arg(long, global = true)]
    root: Option<PathBuf>,

    /// Print JSON instead of text.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate the whole store.
    Check {
        /// Fail on warnings too (component drift).
        #[arg(long)]
        strict: bool,
        /// List every parse error in the code, one per line, instead of checking the store.
        #[arg(long)]
        syntax: bool,
    },
    /// Print an item, or one section with `<id>#<anchor>`.
    Get {
        id: String,
        /// Cut the answer to about this many tokens (chars / 4); whole lines, first lines kept.
        #[arg(long)]
        budget: Option<usize>,
    },
    /// List items.
    List(ListArgs),
    /// Find sections by words: `<id>#<anchor>  L<lines>  <type>  <snippet>` per hit.
    Search(SearchArgs),
    /// Query the code graph: symbols, uses, dependencies, impact, paths.
    Code {
        #[command(subcommand)]
        command: CodeCommand,
    },
    /// The design of the product: `hld` for the whole, `lld <container>` for one container.
    Arch {
        #[command(subcommand)]
        command: ArchCommand,
    },
    /// Write draft components under stone/arch/scan/ for code no component owns yet.
    Scan {
        /// Print each draft with new, changed or unchanged and write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Delete drafts whose files are now owned by another component.
        #[arg(long)]
        prune: bool,
    },
    /// Propose the folder of each container and component without code, from the project files named like it,
    /// and a component for each shared project the services reference.
    Map {
        /// Save every single proposal as the service's `code` and add the shared-library components.
        #[arg(long)]
        write: bool,
        /// Save a folder for one service, as <ID>=<FOLDER>; repeat for more.
        #[arg(long, value_name = "ID=FOLDER")]
        pick: Vec<String>,
    },
    /// Print the trailers for the task in progress, or manage the commit-msg hook.
    CommitMsg {
        /// The task to name instead of the only one in progress.
        #[arg(long)]
        task: Option<String>,
        /// Write the opt-in commit-msg hook that requires a `Refs:` trailer.
        #[arg(long, conflicts_with_all = ["uninstall_hook", "check"])]
        install_hook: bool,
        /// Remove the hook if stone wrote it.
        #[arg(long, conflicts_with = "check")]
        uninstall_hook: bool,
        /// Fail unless the message in FILE has a `Refs:` trailer (what the hook runs).
        #[arg(long, value_name = "FILE")]
        check: Option<PathBuf>,
    },
    /// Explain a file or line: the commits, tasks, plans and decisions behind it.
    Why {
        /// `path` or `path:line`.
        target: String,
        /// Cut the answer to about this many tokens.
        #[arg(long)]
        budget: Option<usize>,
    },
    /// Finished tasks that made the same kind of change as a task, a component or a role.
    Precedents {
        /// The task to find precedents for.
        #[arg(required_unless_present_any = ["component", "role"], conflicts_with_all = ["component", "role"])]
        task: Option<String>,
        /// Look up a component (an architecture id) instead of a task.
        #[arg(long)]
        component: Option<String>,
        /// Look up a role instead of a task: entry point, test, or a symbol kind.
        #[arg(long)]
        role: Option<String>,
        /// Most precedents to show.
        #[arg(long, default_value_t = precedents::DEFAULT_LIMIT)]
        limit: usize,
        /// Cut the answer to about this many tokens.
        #[arg(long)]
        budget: Option<usize>,
    },
    /// The commits that touched an item (or one section, `<id>#<anchor>`), newest first, with their diffs.
    History {
        id: String,
        /// Most commits to show.
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Leave the diffs out.
        #[arg(long)]
        no_diff: bool,
        /// Cut the answer to about this many tokens (chars / 4); whole lines, first lines kept.
        #[arg(long)]
        budget: Option<usize>,
    },
    /// Commits, finished tasks, accepted decisions and doc edits, newest first.
    Timeline {
        /// Oldest day to show: YYYY-MM-DD or Nd (N days back).
        #[arg(long)]
        since: Option<String>,
        /// Newest day to show: YYYY-MM-DD or Nd.
        #[arg(long)]
        until: Option<String>,
        /// Only events under this component's code or items that list it.
        #[arg(long)]
        component: Option<String>,
        /// commit, task_done, decision or doc; repeat for more.
        #[arg(long = "kind")]
        kinds: Vec<String>,
        /// Most events to show.
        #[arg(long, default_value_t = 30)]
        limit: usize,
        /// Cut the answer to about this many tokens (chars / 4); whole lines, first lines kept.
        #[arg(long)]
        budget: Option<usize>,
    },
    /// Create the stone/ folder in the current directory.
    Init {
        /// Task id prefix, such as CS (default TASK).
        #[arg(long)]
        prefix: Option<String>,
        /// Make `stone task move <id> done` refuse a task without a plan and an outcome section.
        #[arg(long)]
        require_plan: bool,
        /// Add the stone block to CLAUDE.md or AGENTS.md without asking (otherwise init asks on a terminal).
        #[arg(long)]
        agent_block: bool,
    },
    /// Create an item.
    New(NewArgs),
    /// Change fields or sections of an item.
    Edit(EditArgs),
    /// Add targets to an item's links.
    Link {
        from: String,
        #[arg(required = true)]
        to: Vec<String>,
        /// Print the resulting file and write nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Task workflow.
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Change stone/config.yml without editing the file; its comments and other lines stay.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Give a numbered item the next free number and update the links to it.
    Renumber {
        /// File to renumber, such as stone/tasks/CS-07.md.
        path: String,
        /// Print what would change and write nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Give a doc, plan, reference or architecture item a new id and point every link to it at the new id.
    Rename {
        /// Id of the item, such as docs/old-name.
        id: String,
        /// The new id in the same folder, such as docs/new-name.
        new_id: String,
        /// Print what would change and write nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete an item of any type; refused while another item links to it (and while a commit names a task).
    Delete {
        id: String,
        /// Delete although items link to it or a commit names it; the broken links are listed.
        #[arg(long)]
        force: bool,
        /// Check and say what would happen; delete nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Serve the read-only MCP tools (search, get, list, code) on stdin and stdout, for clients that cannot run a shell.
    Mcp,
    /// Serve the local web site and its JSON API.
    Serve {
        #[arg(long, default_value_t = 4747)]
        port: u16,
        /// Address to listen on; 0.0.0.0 or :: serves the LAN, and anything but loopback has no authentication.
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Extra host name to accept besides loopback and IP addresses; repeat for more.
        #[arg(long, value_name = "NAME")]
        allow_host: Vec<String>,
        /// Open the page in the default browser.
        #[arg(long)]
        open: bool,
        /// Do not watch files for changes.
        #[arg(long)]
        no_watch: bool,
    },
    /// Print the workflow guide for agents: overview (default), plan, do, finish, debug or design.
    Instructions {
        /// One of overview, plan, do, finish, debug or design.
        topic: Option<String>,
        /// Add the stone block to CLAUDE.md or AGENTS.md, or refresh it between its markers.
        #[arg(long)]
        write: bool,
        /// Cut the answer to about this many tokens (chars / 4); whole lines, first lines kept.
        #[arg(long)]
        budget: Option<usize>,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Add a folder to `code_roots`, such as `src` or `.` for the whole repo.
    AddCodeRoot {
        /// A folder inside the repo, relative to its root.
        path: String,
        /// Print the resulting file and write nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Add a SCIP index file to `scip_indexes`. Config holds data only: you build the index yourself.
    AddScipIndex {
        /// The index file, relative to the repo root, such as `.scip/java.scip`.
        file: String,
        /// The language the index covers, such as `java`.
        #[arg(long)]
        language: String,
        /// File extensions of that language, without a dot, separated by commas, such as `java,kt`.
        #[arg(long, required = true, value_delimiter = ',')]
        extensions: Vec<String>,
        /// Print the resulting file and write nothing.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum TaskCommand {
    /// Set a task's status.
    Move {
        id: String,
        status: String,
        /// Print the resulting file and write nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// List the tasks that are ready to start.
    Next {
        #[arg(long, default_value_t = 5)]
        limit: usize,
    },
    /// Write `closed` into every done task that has none, from its commits or its file; a second run fills nothing.
    FillClosed {
        /// List the tasks and write nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete a task made by mistake; refused while an item links to it or a commit names it (archive it then).
    Remove {
        id: String,
        /// Check and say what would happen; delete nothing.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Args)]
struct NewArgs {
    /// doc, plan, task, milestone, decision, reference, system, container or component.
    item_type: String,
    #[arg(long, allow_hyphen_values = true, required_unless_present = "from")]
    title: Option<String>,
    #[arg(long)]
    slug: Option<String>,
    /// Make the item from a whole markdown file: the title is its front matter `title`, else the
    /// first `# ` heading, else the file name; the body is kept as written. It keeps `description`,
    /// `tags`, `links` and `stale_after`, and warns about other front matter fields.
    #[arg(long, value_name = "FILE")]
    from: Option<PathBuf>,
    /// A field as key=value; repeat for more.
    #[arg(long, value_parser = parse_assignment)]
    set: Vec<(String, String)>,
    /// Link the new item to this item; repeat for more. Adds to the links of a `--from` file.
    #[arg(long, value_name = "ID")]
    link: Vec<String>,
    #[arg(long, requires = "text")]
    section: Option<String>,
    #[arg(long, requires = "section", allow_hyphen_values = true)]
    text: Option<String>,
    /// Print the file and write nothing.
    #[arg(long)]
    dry_run: bool,
}

/// The request `stone new` sends: from the flags alone, or from a markdown file the flags then adjust.
fn new_request(args: NewArgs) -> anyhow::Result<NewItem> {
    let mut req = match &args.from {
        None => NewItem {
            type_name: args.item_type.clone(),
            ..NewItem::default()
        },
        Some(path) => {
            let text = std::fs::read(path).map_err(|err| anyhow::anyhow!("cannot read {}: {err}", path.display()))?;
            let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
            let mut req = NewItem::from_markdown(
                &args.item_type,
                &String::from_utf8_lossy(&text),
                stem.as_deref().unwrap_or_default(),
            )?;
            // An explicit title names the item itself, so the file name no longer decides its slug.
            if args.title.is_some() {
                req.slug = None;
            }
            req
        }
    };
    if let Some(title) = args.title {
        req.title = title;
    }
    if args.slug.is_some() {
        req.slug = args.slug;
    }
    req.set
        .extend(args.set.into_iter().map(|(key, value)| (key, SetValue::from(value))));
    req.links.extend(args.link);
    req.sections = args.section.zip(args.text).into_iter().collect();
    Ok(req)
}

#[derive(Args)]
struct EditArgs {
    id: String,
    /// A change as key=value (lists: a,b replaces, +a adds, -a removes); repeat for more.
    #[arg(long, value_parser = parse_assignment)]
    set: Vec<(String, String)>,
    #[arg(long)]
    unset: Vec<String>,
    /// Replace this section; the text comes from --text or, without it, from stdin. `body` names the text before the first heading.
    #[arg(long)]
    section: Option<String>,
    #[arg(long, requires = "section", allow_hyphen_values = true)]
    text: Option<String>,
    /// Tick the Nth checkbox under Acceptance criteria; repeat for more.
    #[arg(long)]
    check_ac: Vec<usize>,
    /// Add a dated bullet to ## Notes; the same text twice adds it once.
    #[arg(long, allow_hyphen_values = true)]
    note: Option<String>,
    /// Replace the section although it holds sub-sections the new text does not carry.
    #[arg(long, requires = "section")]
    replace_nested: bool,
    /// Print the resulting file and write nothing.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args)]
struct ListArgs {
    #[arg(long = "type")]
    item_type: Option<String>,
    #[arg(long)]
    status: Option<String>,
    #[arg(long)]
    milestone: Option<String>,
    #[arg(long)]
    component: Option<String>,
    /// Only items that need another look (docs, references and arch items).
    #[arg(long)]
    stale: bool,
    /// Cut the answer to about this many tokens (chars / 4); whole lines, first lines kept.
    #[arg(long)]
    budget: Option<usize>,
}

#[derive(Args)]
struct SearchArgs {
    /// Words to find; several words must all match.
    #[arg(required = true)]
    query: Vec<String>,
    /// Rank sections of items linked to this one first.
    #[arg(long)]
    from: Option<String>,
    #[arg(long)]
    component: Option<String>,
    #[arg(long = "type")]
    item_type: Option<String>,
    #[arg(long)]
    status: Option<String>,
    #[arg(long)]
    milestone: Option<String>,
    #[arg(long, default_value_t = 10)]
    limit: usize,
    /// Stop adding hits once the printed lines cost this many tokens (chars / 4).
    #[arg(long)]
    budget: Option<usize>,
}

/// The C4 level `stone arch c4` draws.
#[derive(Clone, Copy, ValueEnum)]
enum C4Level {
    Context,
    Container,
    Component,
}

#[derive(Subcommand)]
enum ArchCommand {
    /// The high-level design: systems and containers, how they connect, and the docs marked `level: hld`.
    Hld {
        /// Cut the answer to about this many tokens (chars / 4); whole lines, first lines kept.
        #[arg(long)]
        budget: Option<usize>,
    },
    /// The low-level design of one container: its service view and the docs marked `level: lld` that link it.
    Lld {
        /// A container id such as arch/api.
        container: String,
        /// Cut the answer to about this many tokens (chars / 4); whole lines, first lines kept.
        #[arg(long)]
        budget: Option<usize>,
    },
    /// What the design lacks: containers with no code, code with no owner, parts with no connection or doc,
    /// connections with no phrase, and written links the code contradicts, each with the command that works on it.
    Gaps {
        /// Cut the answer to about this many tokens (chars / 4); whole lines, first lines kept.
        #[arg(long)]
        budget: Option<usize>,
    },
    /// A C4 diagram as Mermaid text, drawn from the arch items and their connections.
    C4 {
        /// `context` (default), `container` or `component`.
        #[arg(value_enum, default_value = "context")]
        level: C4Level,
        /// For `container`, the system (optional with one product); for `component`, the container.
        id: Option<String>,
        /// Cut the answer to about this many tokens (chars / 4); whole lines, first lines kept.
        #[arg(long)]
        budget: Option<usize>,
    },
    /// Write one connection: the part it starts at, the part it reaches, and what it does.
    Connect {
        /// The part the connection starts at, such as arch/api.
        from: String,
        /// The part the connection reaches.
        to: String,
        /// What the connection does, as a short verb phrase such as `Saves items`.
        #[arg(long)]
        does: String,
        /// The protocol or stack it runs over, such as HTTP or NATS. Leave it out for a call inside one process.
        #[arg(long)]
        over: Option<String>,
        /// Print the resulting file and write nothing.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum CodeCommand {
    /// Symbols by qualified name, exact name or prefix.
    Find {
        name: String,
        /// Only symbols of this kind (type, method, function, member, module, file, project);
        /// class, interface, record, struct and enum mean type, field means member, and a C#
        /// property is a method.
        #[arg(long)]
        kind: Option<String>,
        #[command(flatten)]
        opts: CodeOpts,
    },
    /// What a file declares, nested and in line order; or a type and everything inside it.
    Outline {
        /// A file path, or a symbol (a type shows its members).
        target: String,
        #[arg(long, default_value_t = 200)]
        limit: usize,
        /// Show each signature as written instead of its short form.
        #[arg(long)]
        full: bool,
        /// Stop adding lines once they cost this many tokens (chars / 4).
        #[arg(long)]
        budget: Option<usize>,
    },
    /// Who calls, imports, inherits or references a symbol or file.
    Uses {
        /// A qualified name, a unique name, a file path or path:line.
        target: String,
        #[command(flatten)]
        opts: CodeOpts,
    },
    /// What a symbol or file imports and calls.
    Deps {
        target: String,
        #[command(flatten)]
        opts: CodeOpts,
    },
    /// Everything that depends on a symbol or file, transitively, with the components, tasks and docs it touches.
    Impact {
        target: String,
        /// How many hops to follow (default 3).
        #[arg(long)]
        depth: Option<usize>,
        #[command(flatten)]
        opts: CodeOpts,
    },
    /// The shortest chain of edges between two symbols or files.
    Path {
        from: String,
        to: String,
        #[command(flatten)]
        opts: CodeOpts,
    },
    /// A component: entry points, files, key symbols, dependencies, tasks and docs.
    Explain {
        /// A component id such as arch/core.
        target: String,
        #[command(flatten)]
        opts: CodeOpts,
    },
    /// Where execution starts: declared and detected entry points per component.
    Entries {
        /// A component id such as arch/core; every component when left out.
        target: Option<String>,
        #[command(flatten)]
        opts: CodeOpts,
    },
    /// Exact edges from the language SDKs' SCIP indexers (rust-analyzer, scip-typescript, scip-dotnet, scip-python, scip-dart).
    Native {
        /// Read this SCIP index instead of running the indexers found on PATH. Repeat for several.
        #[arg(long = "index", value_name = "FILE")]
        index: Vec<PathBuf>,
        /// Stop an indexer that runs longer than this many seconds (default 600).
        #[arg(long, value_name = "SECS")]
        timeout: Option<u64>,
        /// Run every indexer everywhere, also where nothing changed since the last pass.
        #[arg(long, conflicts_with = "repo")]
        all: bool,
        /// Run the indexers only in this repo (its folder); the other repos keep their links.
        #[arg(long, value_name = "NAME")]
        repo: Option<String>,
    },
}

#[derive(Args)]
struct CodeOpts {
    /// Leave out inferred edges.
    #[arg(long)]
    exact: bool,
    #[arg(long, default_value_t = 20)]
    limit: usize,
    /// Stop adding hits once the printed lines cost this many tokens (chars / 4).
    #[arg(long)]
    budget: Option<usize>,
}

impl CodeCommand {
    /// The engine query. Text output counts its own cost, so only `--json` hands the budget on.
    fn query(&self, json: bool) -> CodeQuery {
        let all = String::new();
        let (target, to, depth, kind, opts) = match self {
            CodeCommand::Find { name, kind, opts } => (name, None, None, kind.clone(), opts),
            CodeCommand::Entries { target, opts } => (target.as_ref().unwrap_or(&all), None, None, None, opts),
            CodeCommand::Uses { target, opts }
            | CodeCommand::Deps { target, opts }
            | CodeCommand::Explain { target, opts } => (target, None, None, None, opts),
            CodeCommand::Impact { target, depth, opts } => (target, None, *depth, None, opts),
            CodeCommand::Path { from, to, opts } => (from, Some(to.clone()), None, None, opts),
            CodeCommand::Native { .. } => return CodeQuery::default(),
            CodeCommand::Outline { target, limit, .. } => {
                return CodeQuery {
                    target: target.clone(),
                    limit: *limit,
                    ..CodeQuery::default()
                };
            }
        };
        CodeQuery {
            target: target.clone(),
            to,
            depth,
            kind,
            exact_only: opts.exact,
            limit: opts.limit,
            budget: if json { opts.budget } else { None },
        }
    }

    fn budget(&self) -> Option<usize> {
        match self {
            CodeCommand::Find { opts, .. }
            | CodeCommand::Uses { opts, .. }
            | CodeCommand::Deps { opts, .. }
            | CodeCommand::Impact { opts, .. }
            | CodeCommand::Path { opts, .. }
            | CodeCommand::Entries { opts, .. }
            | CodeCommand::Explain { opts, .. } => opts.budget,
            CodeCommand::Outline { budget, .. } => *budget,
            CodeCommand::Native { .. } => None,
        }
    }
}

fn run_code(repo: &Repo, command: &CodeCommand, json: bool) -> anyhow::Result<String> {
    let q = command.query(json);
    let budget = command.budget();
    Ok(match command {
        CodeCommand::Find { .. } => render_code(&code::find(repo, &q)?, json, Detail::None, budget)?,
        CodeCommand::Outline { full, .. } => render_outline(&code::outline(repo, &q)?, json, *full, budget)?,
        CodeCommand::Uses { .. } => linked_or_none(
            code::uses(repo, &q)?,
            |name| format!("nothing uses {name}"),
            json,
            budget,
        )?,
        CodeCommand::Deps { .. } => linked_or_none(
            code::deps(repo, &q)?,
            |name| format!("{name} depends on nothing"),
            json,
            budget,
        )?,
        CodeCommand::Impact { .. } => render_impact(&code_graph::impact(repo, &q)?, json, budget)?,
        CodeCommand::Path { .. } => render_path(&code_graph::path(repo, &q)?, json, budget)?,
        CodeCommand::Explain { .. } => render_explain(&code_graph::explain(repo, &q)?, json, budget)?,
        CodeCommand::Entries { .. } => render_entries(&code_entries::entries(repo, &q)?, json, budget)?,
        CodeCommand::Native {
            index,
            timeout,
            all,
            repo: only,
        } => {
            let query = NativeQuery {
                indexes: index.clone(),
                search_path: None,
                timeout: timeout.map(std::time::Duration::from_secs),
                all: *all,
                repo: only.clone(),
            };
            if json {
                render_native(&code_native::native(repo, &query)?, true)?
            } else {
                let tty = std::io::stderr().is_terminal();
                let report = code_native::native_with(repo, &query, &mut |step| native_progress(&step, tty))?;
                clear_status(tty);
                native_summary(&report)
            }
        }
    })
}

/// A pass as it goes: each indexer's line on stdout when it ends; what runs now on stderr, as
/// one line rewritten in place on a terminal and as plain lines otherwise.
fn native_progress(step: &NativeStep, tty: bool) {
    let now = match step {
        NativeStep::Started {
            indexer,
            dir,
            run,
            runs,
        } => format!("{indexer}: indexing {dir} ({run} of {runs})"),
        NativeStep::Running {
            indexer,
            dir,
            secs,
            last,
        } => {
            if !tty {
                return;
            }
            let last: String = last.chars().take(60).collect();
            format!("{indexer}: indexing {dir}, {secs}s  {last}")
        }
        NativeStep::Finished { run } => {
            clear_status(tty);
            print!("{}", native_line(run));
            let _ = std::io::stdout().flush();
            return;
        }
        NativeStep::Storing => "storing the edges".to_string(),
    };
    if tty {
        let now: String = now.chars().take(110).collect();
        eprint!("\r\x1b[2K{now}");
    } else {
        eprintln!("{now}");
    }
}

fn clear_status(tty: bool) {
    if tty {
        eprint!("\r\x1b[2K");
    }
}

/// Starts `stone code native` on its own when files changed since the last pass, so the exact
/// edges catch up while the next task begins. Queries meanwhile show the out-of-date note. It
/// writes to `.stone/native/background.log`; a failure to start is only told, never an error.
fn refresh_native_in_background(repo: &Repo) {
    if !code_native::native_due(repo).unwrap_or(false) {
        return;
    }
    let root = &repo.store.layout.root;
    let started = std::env::current_exe().and_then(|exe| {
        let log = std::fs::File::create(root.join(".stone").join("native").join("background.log"))?;
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("--root")
            .arg(root)
            .args(["code", "native"])
            .stdin(std::process::Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP: a console of its own, hidden, so it
            // outlives this command and its console, and the indexers it starts share the hidden
            // console instead of each opening a window (as they do under DETACHED_PROCESS).
            cmd.creation_flags(0x0800_0000 | 0x0000_0200);
        }
        cmd.spawn()
    });
    match started {
        Ok(_) => eprintln!("exact links are refreshing in the background"),
        Err(err) => eprintln!("could not refresh exact links in the background: {err}; run `stone code native`"),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("✗ {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// An explicit `--root` is the repository root as given. Only the current directory walks up to an
/// ancestor store, so a write can never land in a store the owner did not name.
fn open_repo(cli_root: Option<&std::path::Path>, start: &std::path::Path) -> anyhow::Result<Repo> {
    if let Some(root) = cli_root
        && !root.join("stone").join("config.yml").is_file()
    {
        anyhow::bail!(
            "{} holds no store; run stone init there, or pass the repository root",
            root.display()
        );
    }
    Ok(Repo::open(start)?)
}

fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    if let Command::Instructions { topic, write, budget } = &cli.command {
        let start = match &cli.root {
            Some(root) => root.clone(),
            None => std::env::current_dir()?,
        };
        if *write {
            let done = open_repo(cli.root.as_deref(), &start)?.write_agent_block()?;
            print!("{}", render_agent_block(&done, cli.json)?);
            return Ok(ExitCode::SUCCESS);
        }
        // The guide needs no store, so it prints even before `stone init`.
        print!("{}", lean::fit_lines(*budget, &instructions(topic.as_deref(), &start)?));
        return Ok(ExitCode::SUCCESS);
    }
    if let Command::CommitMsg { check: Some(file), .. } = &cli.command {
        // The hook runs this in any checkout, so it reads the message and nothing else.
        let text = std::fs::read(file).map_err(|err| anyhow::anyhow!("cannot read {}: {err}", file.display()))?;
        commit_msg::check_message(&String::from_utf8_lossy(&text))?;
        return Ok(ExitCode::SUCCESS);
    }
    let start = match &cli.root {
        Some(root) => root.clone(),
        None => std::env::current_dir()?,
    };
    if let Command::Init {
        prefix,
        require_plan,
        agent_block,
    } = &cli.command
    {
        // No store exists yet, so this is the one command that must not open one.
        let done = init_with(&start, prefix.as_deref(), *require_plan)?;
        let block = offer_agent_block(&start, *agent_block, cli.json)?;
        print!("{}", render_init(&done, &block, cli.json)?);
        return Ok(ExitCode::SUCCESS);
    }
    let repo = open_repo(cli.root.as_deref(), &start)?;
    match cli.command {
        Command::Mcp => {
            // The transport owns stdout; only stderr may carry diagnostics here.
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
            runtime.block_on(carvestone_mcp::serve_stdio(repo))?;
        }
        Command::Serve {
            port,
            host,
            allow_host,
            open,
            no_watch,
        } => {
            let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
            let opts = ServeOpts {
                host,
                port,
                allow_host,
                watch: !no_watch,
                open,
            };
            runtime.block_on(carvestone_server::serve(repo, opts))?;
        }
        Command::Check { strict, syntax } => {
            let problems = if syntax { repo.syntax_sites()? } else { repo.check()? };
            print!("{}", render_problems(&problems, cli.json)?);
            if problems.iter().any(|p| strict || p.severity == Severity::Error) {
                return Ok(ExitCode::FAILURE);
            }
        }
        Command::Get { id, budget } => {
            let text = render_get(&repo.get(&id)?, cli.json)?;
            print!("{}", lean::fit_lines(if cli.json { None } else { budget }, &text));
        }
        Command::List(args) => {
            let filter = ListFilter {
                item_type: args.item_type,
                status: args.status,
                milestone: args.milestone,
                assignee: None,
                component: args.component,
                stale: args.stale,
                changed_files: cli.json,
            };
            let (type_known, status_known) = (filter.item_type.is_some(), filter.status.is_some());
            let rows = repo.list(&filter)?;
            if rows.is_empty() && !cli.json {
                println!("no items match");
            } else {
                let text = render_rows(&rows, cli.json, type_known, status_known)?;
                print!("{}", lean::fit_lines(if cli.json { None } else { args.budget }, &text));
            }
        }
        Command::Search(args) => {
            let query = SearchQuery {
                text: args.query.join(" "),
                from: args.from,
                component: args.component,
                kind: args.item_type,
                status: args.status,
                milestone: args.milestone,
                limit: args.limit,
                // Text output counts its own cost; only `--json` hands the budget on.
                budget: if cli.json { args.budget } else { None },
            };
            match search(&repo, &query) {
                Ok(result) => {
                    let titles = if cli.json {
                        HashMap::new()
                    } else {
                        item_titles(&repo, &result)?
                    };
                    print!("{}", render_search(&result, cli.json, &titles, args.budget)?);
                }
                Err(err @ EngineError::EmptyQuery) => {
                    eprintln!("✗ {err}");
                    return Ok(ExitCode::from(2));
                }
                Err(err) => return Err(err.into()),
            }
        }
        Command::Code { command } => print!("{}", run_code(&repo, &command, cli.json)?),
        Command::Arch {
            command:
                ArchCommand::Connect {
                    from,
                    to,
                    does,
                    over,
                    dry_run,
                },
        } => {
            show(
                &repo.connect(&from, &to, &does, over.as_deref(), dry_run)?,
                Shown::Edited,
                dry_run,
                cli.json,
            )?;
        }
        Command::Arch { command } => print!("{}", run_arch(&repo, &command, cli.json)?),
        Command::New(args) => {
            let dry_run = args.dry_run;
            let req = new_request(args)?;
            show(&repo.create(&req, dry_run)?, Shown::Created, dry_run, cli.json)?;
        }
        Command::Edit(args) => {
            let section = match args.section {
                Some(name) => {
                    let text = match args.text {
                        Some(text) => text,
                        None => {
                            let piped = read_stdin()?;
                            // A forgotten --text would otherwise blank the section.
                            if piped.trim().is_empty() {
                                anyhow::bail!("no text for section `{name}`: pass --text or pipe the text on stdin");
                            }
                            piped
                        }
                    };
                    Some((name, text))
                }
                None => None,
            };
            let req = Edit {
                set: args
                    .set
                    .into_iter()
                    .map(|(key, value)| (key, SetValue::from(value)))
                    .collect(),
                unset: args.unset,
                section,
                replace_nested: args.replace_nested,
                check_ac: args.check_ac,
                note: args.note,
            };
            show(
                &repo.edit(&args.id, &req, args.dry_run)?,
                Shown::Edited,
                args.dry_run,
                cli.json,
            )?;
        }
        Command::Link { from, to, dry_run } => {
            show(&repo.link(&from, &to, dry_run)?, Shown::Edited, dry_run, cli.json)?;
        }
        Command::Config {
            command: ConfigCommand::AddCodeRoot { path, dry_run },
        } => {
            show(&repo.add_code_root(&path, dry_run)?, Shown::Edited, dry_run, cli.json)?;
        }
        Command::Config {
            command:
                ConfigCommand::AddScipIndex {
                    file,
                    language,
                    extensions,
                    dry_run,
                },
        } => {
            show(
                &repo.add_scip_index(&file, &language, &extensions, dry_run)?,
                Shown::Edited,
                dry_run,
                cli.json,
            )?;
        }
        Command::Task {
            command: TaskCommand::Move { id, status, dry_run },
        } => {
            show(
                &repo.move_task(&id, &status, dry_run)?,
                Shown::Edited,
                dry_run,
                cli.json,
            )?;
            if status == "done" && !dry_run {
                refresh_native_in_background(&repo);
            }
        }
        Command::Task {
            command: TaskCommand::Next { limit },
        } => {
            let mut rows = repo.next(usize::MAX)?;
            let more = rows.len().saturating_sub(limit);
            rows.truncate(limit);
            let mut text = render_rows(&rows, cli.json, true, true)?;
            if !cli.json {
                if rows.is_empty() && more == 0 {
                    text = "no task is ready: none is todo with all its depends_on done\n".to_string();
                } else if more > 0 {
                    text.push_str(&format!("{more} more ready; raise --limit\n"));
                }
            }
            print!("{text}");
        }
        Command::Task {
            command: TaskCommand::FillClosed { dry_run },
        } => {
            let filled = repo.fill_closed(dry_run)?;
            print!("{}", render_filled(&filled, dry_run, cli.json)?);
        }
        Command::Task {
            command: TaskCommand::Remove { id, dry_run },
        } => {
            let done = repo.remove_task(&id, dry_run)?;
            print!("{}", render_removed(&done, dry_run, cli.json)?);
        }
        Command::Renumber { path, dry_run } => {
            let done = repo.renumber(&path, dry_run)?;
            for warning in &done.warnings {
                eprintln!("warning: {warning}");
            }
            print!("{}", render_renumbered(&done, dry_run, cli.json)?);
        }
        Command::Rename { id, new_id, dry_run } => {
            let done = repo.rename_item(&id, &new_id, dry_run)?;
            print!("{}", render_renamed(&done, dry_run, cli.json)?);
        }
        Command::Delete { id, force, dry_run } => {
            let done = repo.delete_item(&id, dry_run, force)?;
            print!("{}", render_removed(&done, dry_run, cli.json)?);
        }
        Command::Scan { dry_run, prune } => {
            let report = scan(&repo, &ScanOpts { dry_run, prune })?;
            print!("{}", render_scan(&report, dry_run, cli.json)?);
        }
        Command::Map { write, pick } => {
            let proposals = repo.service_code()?;
            let mut picks = Vec::new();
            for raw in &pick {
                let Some((id, folder)) = raw.split_once('=') else {
                    anyhow::bail!("--pick takes <ID>=<FOLDER>, like arch/api=services/api; got `{raw}`");
                };
                picks.push((id.trim().to_string(), folder.trim().to_string()));
            }
            if write {
                for p in &proposals {
                    if let Some(folder) = &p.folder
                        && !picks.iter().any(|(id, _)| *id == p.id)
                    {
                        picks.push((p.id.clone(), folder.clone()));
                    }
                }
            }
            let wrote = write || !pick.is_empty();
            let pending: Vec<(String, String)> = proposals
                .iter()
                .filter_map(|p| Some((p.id.clone(), p.folder.clone()?)))
                .collect();
            let mut written = repo.set_service_code(&picks)?;
            // Once the folders are saved, the services read them from their items.
            let libraries = repo.library_code(if wrote { &[] } else { &pending })?;
            let added = if write {
                repo.add_libraries(&libraries)?
            } else {
                Vec::new()
            };
            written.extend(added.iter().cloned());
            print!(
                "{}",
                render_map(&proposals, &libraries, &written, &added, wrote, cli.json)?
            );
        }
        Command::CommitMsg {
            task,
            install_hook,
            uninstall_hook,
            ..
        } => print!(
            "{}",
            run_commit_msg(&repo, task.as_deref(), install_hook, uninstall_hook, cli.json)?
        ),
        Command::Why { target, budget } => {
            let (path, line) = why::parse_target(&target);
            print!("{}", render_why(&why::why(&repo, &path, line, budget)?, cli.json)?);
        }
        Command::Precedents {
            task,
            component,
            role,
            limit,
            budget,
        } => {
            let lookup = Lookup {
                task,
                component,
                role,
                limit: Some(limit),
                budget,
            };
            let found = precedents::precedents(&repo, &lookup)?;
            print!("{}", render_precedents(&found, cli.json)?);
        }
        Command::History {
            id,
            limit,
            no_diff,
            budget,
        } => {
            let opts = HistoryOpts { limit, diff: !no_diff };
            let found = history::history(&repo, &id, &opts)?;
            let text = if cli.json { pretty(&found)? } else { found.text() };
            print!("{}", lean::fit_lines(if cli.json { None } else { budget }, &text));
        }
        Command::Timeline {
            since,
            until,
            component,
            kinds,
            limit,
            budget,
        } => {
            let today = Date::today();
            let opts = TimelineOpts {
                since: since.map(|s| parse_date("--since", &s, today)).transpose()?,
                until: until.map(|s| parse_date("--until", &s, today)).transpose()?,
                component,
                kinds: kinds.iter().map(|k| EventKind::parse(k)).collect::<Result<_, _>>()?,
                limit,
            };
            // One event past the limit tells whether older ones were left out.
            let shown = if limit == 0 { 30 } else { limit };
            let mut events = timeline::timeline(
                &repo,
                &TimelineOpts {
                    limit: shown + 1,
                    ..opts
                },
            )?;
            let more = events.len() > shown;
            events.truncate(shown);
            let text = render_timeline(&events, more, cli.json)?;
            print!("{}", lean::fit_lines(if cli.json { None } else { budget }, &text));
        }
        Command::Init { .. } | Command::Instructions { .. } => {}
    }
    Ok(ExitCode::SUCCESS)
}

fn run_commit_msg(
    repo: &Repo,
    task: Option<&str>,
    install: bool,
    uninstall: bool,
    json: bool,
) -> anyhow::Result<String> {
    if install {
        // One line per repo found; the repo is named only when there are several, since with one it adds nothing.
        let done = commit_msg::install_hooks(repo)?;
        let several = done.len() > 1;
        let text = done
            .iter()
            .map(|d| {
                let place = if several {
                    format!(" in {}", commit_msg::repo_label(&d.hook.repo))
                } else {
                    String::new()
                };
                match d.state {
                    HookState::Installed => format!("hook installed{place}: {}", d.hook.path.display()),
                    HookState::AlreadyInstalled => format!("hook already installed{place}"),
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let hooks: Vec<_> = done
            .iter()
            .map(|d| {
                serde_json::json!({
                    "repo": d.hook.repo,
                    "hook": d.hook.path,
                    "installed": d.state == HookState::Installed,
                })
            })
            .collect();
        return plain_or_json(
            &text,
            json,
            serde_json::json!({ "hook": done[0].hook.path, "installed": true, "hooks": hooks }),
        );
    }
    if uninstall {
        let done = commit_msg::uninstall_hooks(repo)?;
        let several = done.len() > 1;
        let text = done
            .iter()
            .map(|d| {
                let place = if several {
                    format!(" in {}", commit_msg::repo_label(&d.hook.repo))
                } else {
                    String::new()
                };
                match d.state {
                    UninstallState::Removed => format!("hook removed{place}"),
                    UninstallState::NotInstalled => format!("no hook installed{place}"),
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        return plain_or_json(&text, json, serde_json::json!({ "message": text }));
    }
    let lines = commit_msg::trailers_for(repo, task)?;
    if json {
        return pretty(&lines);
    }
    Ok(with_newline(lines.join("\n")))
}

fn plain_or_json(text: &str, json: bool, value: serde_json::Value) -> anyhow::Result<String> {
    if json {
        return pretty(&value);
    }
    Ok(with_newline(text.to_string()))
}

fn render_timeline(events: &[Event], more: bool, json: bool) -> anyhow::Result<String> {
    if json {
        return pretty(&events);
    }
    if events.is_empty() {
        return Ok("no events\n".to_string());
    }
    let mut out = with_newline(events.iter().map(Event::line).collect::<Vec<_>>().join("\n"));
    if more {
        out.push_str("older events were left out; raise --limit or narrow with --since\n");
    }
    Ok(out)
}

fn render_why(result: &Why, json: bool) -> anyhow::Result<String> {
    if json {
        return pretty(result);
    }
    Ok(with_newline(result.lines().join("\n")))
}

fn render_precedents(result: &Precedents, json: bool) -> anyhow::Result<String> {
    if json {
        return pretty(result);
    }
    Ok(with_newline(result.lines().join("\n")))
}

/// The guide text for an optional topic name; no topic prints the overview. The store at `start`
/// may replace a topic in `docs/instructions`; otherwise the text is built in.
fn instructions(topic: Option<&str>, start: &std::path::Path) -> anyhow::Result<String> {
    let topic = match topic {
        None => Topic::Overview,
        Some(name) => Topic::from_name(name).ok_or_else(|| anyhow::anyhow!("{}", unknown_topic(name)))?,
    };
    Ok(guide_text(start, topic))
}

fn with_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn render_problems(problems: &[Problem], json: bool) -> anyhow::Result<String> {
    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(problems)?));
    }
    let mut out = String::new();
    for p in problems {
        out.push_str(&format!("{p}\n"));
    }
    let warnings = problems.iter().filter(|p| p.severity == Severity::Warning).count();
    out.push_str(&match problems.len() - warnings {
        0 => "no problems".to_string(),
        1 => "1 problem".to_string(),
        n => format!("{n} problems"),
    });
    match warnings {
        0 => {}
        1 => out.push_str(", 1 warning"),
        n => out.push_str(&format!(", {n} warnings")),
    }
    out.push('\n');
    Ok(out)
}

/// Rows as text; a column the command line already fixed (`--type`, `--status`) is left out.
fn render_rows(rows: &[ListRow], json: bool, type_known: bool, status_known: bool) -> anyhow::Result<String> {
    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(rows)?));
    }
    Ok(lean::list_lines(rows, type_known, status_known))
}

fn render_get(result: &GetResult, json: bool) -> anyhow::Result<String> {
    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(result)?));
    }
    Ok(match result {
        GetResult::Item(view) => {
            let stale = view
                .stale
                .as_ref()
                .map_or_else(String::new, |s| format!("Stale: {}\n", s.reason));
            // The id already says where the file is, unless the layout is not the usual one.
            let name = view.short.as_deref().unwrap_or(&view.id);
            // A doc-root file is named by its path, which is also its id.
            let header = if view.path == format!("stone/{}.md", view.id) || view.path == view.id {
                name.to_string()
            } else {
                format!("{name} ({})", view.path)
            };
            let mut text = format!("{header}\n{stale}{}", lean::item_text(&view.text));
            if let Some(note) = libraries_note(&view.libraries) {
                text.push_str(&note);
                text.push('\n');
            }
            if !view.code_dependencies.is_empty() {
                text.push_str("\nCode dependencies:\n");
                for dep in &view.code_dependencies {
                    text.push_str(&format!("{}  {}\n", dep.to, dep.summary()));
                }
            }
            if let Some(block) = changed_files_block(&view.changed_files) {
                text.push_str(&block);
            }
            text + &render_backlinks(&view.backlinks)
        }
        GetResult::Section(view) => lean::with_newline(&view.text) + &render_backlinks(&view.backlinks),
        GetResult::Code(view) => {
            format!(
                "{} ({}:L{}-{}  {})\n",
                view.id, view.path, view.lines[0], view.lines[1], view.type_name
            ) + &render_backlinks(&view.backlinks)
        }
    })
}

fn render_scan(report: &ScanReport, dry_run: bool, json: bool) -> anyhow::Result<String> {
    if json {
        let entries: Vec<_> = report
            .entries
            .iter()
            .map(|(path, state)| serde_json::json!({ "path": path, "state": state.as_str() }))
            .collect();
        let value = serde_json::json!({ "written": report.written, "entries": entries });
        return Ok(format!("{}\n", serde_json::to_string_pretty(&value)?));
    }
    let mut out = String::new();
    if dry_run {
        for (path, state) in &report.entries {
            out.push_str(&format!("{path}  {}\n", state.as_str()));
        }
        if report.entries.is_empty() {
            out.push_str("nothing to scan: no code root has files to own\n");
        }
        return Ok(out);
    }
    for (path, state) in &report.entries {
        match state {
            ScanState::KeptEdited => out.push_str(&format!("kept {path}: edited\n")),
            ScanState::Stale => out.push_str(&format!(
                "stale {path}: its files are owned elsewhere or gone; run `stone scan --prune` to delete it\n"
            )),
            ScanState::Pruned => out.push_str(&format!("pruned {path}\n")),
            _ => {}
        }
    }
    out.push_str(&format!(
        "{} written, {} unchanged\n",
        report.written,
        report.count(ScanState::Unchanged)
    ));
    Ok(out)
}

fn render_map(
    proposals: &[ServiceCode],
    libraries: &[LibraryCode],
    written: &[Written],
    added: &[Written],
    wrote: bool,
    json: bool,
) -> anyhow::Result<String> {
    if json {
        let saved: Vec<&str> = written.iter().filter(|w| w.changed).map(|w| w.id.as_str()).collect();
        let value = serde_json::json!({ "services": proposals, "libraries": libraries, "written": saved });
        return Ok(format!("{}\n", serde_json::to_string_pretty(&value)?));
    }
    let mut out = String::new();
    if wrote {
        for w in written.iter().filter(|w| w.changed) {
            let what = if added.iter().any(|a| a.id == w.id) {
                "added"
            } else {
                "saved"
            };
            out.push_str(&format!("{what} {}\n", w.id));
        }
        let saved = written.iter().filter(|w| w.changed).count() - added.len();
        let left = proposals.len().saturating_sub(saved);
        if left > 0 {
            out.push_str(&format!("{left} without code; run `stone map` to see them\n"));
        }
        if added.is_empty() && !libraries.is_empty() {
            out.push_str(&format!(
                "{} shared libraries have no component; run `stone map` to see them\n",
                libraries.len()
            ));
        }
        return Ok(out);
    }
    if proposals.is_empty() && libraries.is_empty() {
        return Ok("every container and component names its code\n".to_string());
    }
    for p in proposals {
        match (&p.folder, &p.project) {
            (Some(folder), Some(project)) => {
                out.push_str(&format!("{}  {}  {folder}/**  (from {project})\n", p.id, p.title))
            }
            _ if !p.candidates.is_empty() => out.push_str(&format!(
                "{}  {}  {} folders match: {}; pick one with --pick {}=<folder>\n",
                p.id,
                p.title,
                p.candidates.len(),
                p.candidates.join(", "),
                p.id
            )),
            _ => out.push_str(&format!(
                "{}  {}  no project is named like it; save its folder with --pick {}=<folder>\n",
                p.id, p.title, p.id
            )),
        }
    }
    if !libraries.is_empty() {
        out.push_str("Shared libraries, each a component of its own:\n");
        for l in libraries {
            out.push_str(&format!(
                "{}  {}  {}/**  used by {}\n",
                l.id,
                l.name,
                l.folder,
                l.used_by.join(", ")
            ));
        }
    }
    let ready = proposals.iter().filter(|p| p.folder.is_some()).count() + libraries.len();
    if ready > 0 {
        out.push_str(&format!("run `stone map --write` to save the {ready} proposed\n"));
    }
    Ok(out)
}

/// Titles of the items whose front matter a hit points at, so a hit that only repeats the title can be left out.
fn item_titles(repo: &Repo, result: &SearchResult) -> anyhow::Result<HashMap<String, String>> {
    let front_matter_hit =
        |hit: &carvestone_engine::search::Hit| hit.anchor.is_none() && hit.exactness.is_none() && hit.kind != "symbol";
    if !result.hits.iter().any(front_matter_hit) {
        return Ok(HashMap::new());
    }
    Ok(repo
        .list(&ListFilter::default())?
        .into_iter()
        .filter_map(|row| row.title.map(|title| (row.id, title)))
        .collect())
}

/// Search text: the hits that fit `budget` (everything printed counts), `--json` aside.
fn render_search(
    result: &SearchResult,
    json: bool,
    titles: &HashMap<String, String>,
    budget: Option<usize>,
) -> anyhow::Result<String> {
    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(result)?));
    }
    if result.hits.is_empty() {
        return Ok(format!("no matches for \"{}\"\n", result.query));
    }
    let lines = lean::search_lines(&result.hits, titles);
    Ok(lean::fit(budget, lines.len(), |kept| {
        let mut out = lines[..kept].join("\n") + "\n";
        // Lines the CLI merged stand for hits it printed, so only lines past the budget count.
        out.push_str(&code_tail(result.omitted + (lines.len() - kept), None));
        out
    }))
}

fn run_arch(repo: &Repo, command: &ArchCommand, json: bool) -> anyhow::Result<String> {
    Ok(match command {
        ArchCommand::Hld { budget } => {
            let hld = design::hld(repo)?;
            if json {
                pretty(&hld)?
            } else {
                lean::fit_lines(*budget, &render_hld(&hld))
            }
        }
        ArchCommand::Lld { container, budget } => {
            let lld = design::lld(repo, container)?;
            if json {
                pretty(&lld)?
            } else {
                lean::fit_lines(*budget, &render_lld(&lld)?)
            }
        }
        ArchCommand::Gaps { budget } => {
            let found = carvestone_engine::arch_gaps::gaps(repo)?;
            if json {
                pretty(&found)?
            } else {
                lean::fit_lines(*budget, &render_gaps(&found))
            }
        }
        ArchCommand::C4 { level, id, budget } => {
            let source = match (level, id.as_deref()) {
                (C4Level::Context, None) => c4::c4_context(repo)?,
                (C4Level::Context, Some(_)) => anyhow::bail!("`arch c4 context` takes no id; it draws every system"),
                (C4Level::Container, system) => c4::c4_container(repo, system)?,
                (C4Level::Component, Some(container)) => c4::c4_component(repo, container)?,
                (C4Level::Component, None) => {
                    anyhow::bail!("`arch c4 component` needs a container id; list them with `stone arch hld`")
                }
            };
            if json {
                pretty(&c4::C4Source { source })?
            } else {
                lean::fit_lines(*budget, &source)
            }
        }
        ArchCommand::Connect { .. } => unreachable!("`arch connect` is handled before `run_arch`"),
    })
}

/// One line per gap (kind, id, detail) and the next command indented under it; `no gaps` when clean.
fn render_gaps(found: &carvestone_engine::arch_gaps::Gaps) -> String {
    if found.items.is_empty() {
        return "no gaps\n".to_string();
    }
    let mut out = String::new();
    for gap in &found.items {
        out.push_str(&format!(
            "{}  {}  {}\n  {}\n",
            gap.kind.as_str(),
            gap.id,
            gap.detail,
            gap.next
        ));
    }
    out
}

/// One line per part: `id  Title (technology)  description`.
fn part_line(part: &design::Part) -> String {
    let mut out = format!("{}  {}", part.id, part.title);
    if let Some(technology) = &part.technology {
        out.push_str(&format!(" ({technology})"));
    }
    if let Some(kind) = &part.kind {
        out.push_str(&format!(" [{kind}]"));
    }
    if let Some(description) = &part.description {
        out.push_str(&format!("  {description}"));
    }
    if let Some(parent) = &part.parent {
        out.push_str(&format!("  [in {parent}]"));
    }
    out.push('\n');
    out
}

fn design_docs(title: &str, none: &str, docs: &[design::DesignDoc]) -> String {
    if docs.is_empty() {
        return format!("{title}: none{none}\n");
    }
    let mut out = format!("{title}:\n");
    for doc in docs {
        out.push_str(&format!("{}  {}", doc.id, doc.title));
        if let Some(start) = &doc.start {
            out.push_str(&format!("  starts at {start}"));
        }
        out.push('\n');
    }
    out
}

fn render_hld(hld: &design::Hld) -> String {
    let mut out = String::from("HLD\n");
    for (title, parts) in [("Systems", &hld.systems), ("Containers", &hld.containers)] {
        if parts.is_empty() {
            out.push_str(&format!(
                "{title}: none; add one with `stone new {}`\n",
                title.to_lowercase().trim_end_matches('s')
            ));
            continue;
        }
        out.push_str(&format!("{title}:\n"));
        for part in parts {
            out.push_str(&part_line(part));
        }
    }
    if hld.connections.is_empty() {
        out.push_str("Connections: none; add one with `stone arch connect <from> <to> --does <text>`\n");
    } else {
        out.push_str("Connections:\n");
        for link in &hld.connections {
            match &link.description {
                Some(text) => out.push_str(&format!("{} -> {}: {text}\n", link.from, link.to)),
                None => out.push_str(&format!("{} -> {}\n", link.from, link.to)),
            }
        }
    }
    if hld.drawn.is_empty() {
        out.push_str("Drawn in diagrams: none\n");
    } else {
        out.push_str("Drawn in diagrams:\n");
        for rel in &hld.drawn {
            out.push_str(&format!("{}: {}\n", rel.item, rel.line));
        }
    }
    out.push_str(&design_docs("Docs (hld)", "; mark a doc with `level: hld`", &hld.docs));
    out
}

fn render_lld(lld: &design::Lld) -> anyhow::Result<String> {
    let mut out = format!("LLD {}  {}\n", lld.container.id, lld.container.title);
    match &lld.service {
        Some(service) => out.push_str(&render_explain(service, false, None)?),
        None => out.push_str(&format!(
            "Service view: none; {} has no `code` list, so give it one or run `stone map`\n",
            lld.container.id
        )),
    }
    out.push_str(&design_docs(
        "Docs (lld)",
        " link this container; mark a doc with `level: lld` and link it here",
        &lld.docs,
    ));
    Ok(out)
}

fn pretty<T: serde::Serialize>(value: &T) -> anyhow::Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(value)?))
}

/// The cut-off and still-building lines every code query ends with: `omitted` results left out by
/// the engine and by the printed budget together.
fn code_tail(omitted: usize, note: Option<&str>) -> String {
    let mut out = String::new();
    match omitted {
        0 => {}
        1 => out.push_str("1 more result left out; raise --limit or --budget\n"),
        n => out.push_str(&format!("{n} more results left out; raise --limit or --budget\n")),
    }
    if let Some(note) = note {
        out.push_str(&format!("note: {note}\n"));
    }
    out
}

/// The lines an empty answer adds about what is indexed, each on its own line; nothing when the
/// answer has hits.
fn gap_lines(gap: Option<&str>) -> String {
    gap.map_or_else(String::new, |gap| format!("{gap}\n"))
}

/// `uses` and `deps` as text. They fail on a target that matches nothing, so an empty answer
/// means the symbol is there with no links: said so, not as "no matches".
fn linked_or_none(
    result: CodeResult,
    none: fn(&str) -> String,
    json: bool,
    budget: Option<usize>,
) -> anyhow::Result<String> {
    if result.hits.is_empty() && !json {
        return Ok(format!(
            "{}\n{}{}",
            none(&result.query),
            gap_lines(result.gap.as_deref()),
            code_tail(0, result.note.as_deref())
        ));
    }
    render_code(&result, json, Detail::One, budget)
}

/// Code answers as text. `budget` (tokens, chars / 4) covers everything printed, the cut-off sentence included.
fn render_code(result: &CodeResult, json: bool, detail: Detail, budget: Option<usize>) -> anyhow::Result<String> {
    if json {
        return pretty(result);
    }
    let note = result.note.as_deref();
    if result.hits.is_empty() {
        return Ok(format!(
            "no matches for \"{}\"\n{}{}",
            result.query,
            gap_lines(result.gap.as_deref()),
            code_tail(0, note)
        ));
    }
    Ok(lean::fit(budget, result.hits.len(), |kept| {
        let mut out = lean::symbol_lines(&result.hits[..kept], detail, true, &mut HashSet::new());
        out.push_str(&code_tail(result.omitted + (result.hits.len() - kept), note));
        out
    }))
}

/// `<path>` then one line per declaration, two spaces deeper per level: `kind signature  L<a>-<b>`.
fn render_outline(result: &Outline, json: bool, full: bool, budget: Option<usize>) -> anyhow::Result<String> {
    if json {
        return pretty(result);
    }
    let note = result.note.as_deref();
    if result.entries.is_empty() {
        return Ok(format!(
            "{} declares nothing\n{}{}",
            result.path,
            gap_lines(result.gap.as_deref()),
            code_tail(0, note)
        ));
    }
    Ok(lean::fit(budget, result.entries.len(), |kept| {
        let mut out = format!("{}\n", result.path);
        for entry in &result.entries[..kept] {
            let text = if !full {
                entry.short.as_str()
            } else if entry.signature.is_empty() {
                entry.name.as_str()
            } else {
                entry.signature.as_str()
            };
            // `export function f()` says its kind itself; a label in front would repeat it.
            let said = full
                && entry
                    .signature
                    .split(entry.name.as_str())
                    .next()
                    .is_some_and(|before| before.split_whitespace().any(|word| word == entry.kind));
            let label = if said {
                String::new()
            } else {
                format!("{} ", entry.kind)
            };
            let lines = if entry.lines[0] == entry.lines[1] {
                format!("L{}", entry.lines[0])
            } else {
                format!("L{}-{}", entry.lines[0], entry.lines[1])
            };
            let indent = "  ".repeat(entry.depth as usize + 1);
            out.push_str(&format!("{indent}{label}{text}  {lines}\n"));
        }
        out.push_str(&code_tail(result.omitted + (result.entries.len() - kept), note));
        out
    }))
}

fn render_impact(result: &Impact, json: bool, budget: Option<usize>) -> anyhow::Result<String> {
    if json {
        return pretty(result);
    }
    let mut tail = String::new();
    for (title, list) in [
        ("Components", &result.components),
        ("Tasks", &result.tasks),
        ("Docs", &result.docs),
    ] {
        if !list.is_empty() {
            tail.push_str(&format!("{title}: {}\n", list.join(", ")));
        }
    }
    if result.dependents.is_empty() {
        let note = code_tail(result.omitted, result.note.as_deref());
        return Ok(format!(
            "nothing depends on {}\n{}{tail}{note}",
            result.query,
            gap_lines(result.gap.as_deref())
        ));
    }
    Ok(lean::fit(budget, result.dependents.len(), |kept| {
        let mut out = lean::symbol_lines(&result.dependents[..kept], Detail::Two, true, &mut HashSet::new());
        out.push_str(&tail);
        out.push_str(&code_tail(
            result.omitted + (result.dependents.len() - kept),
            result.note.as_deref(),
        ));
        out
    }))
}

fn render_path(result: &PathResult, json: bool, budget: Option<usize>) -> anyhow::Result<String> {
    if json {
        return pretty(result);
    }
    if result.steps.is_empty() {
        let tail = code_tail(result.omitted, result.note.as_deref());
        return Ok(format!(
            "no path between {} and {}\n{}{tail}",
            result.from,
            result.to,
            gap_lines(result.gap.as_deref())
        ));
    }
    // The chain is walked in order, so its steps keep their order instead of grouping by file.
    let inferred = if result.exact { "" } else { "  inferred" };
    Ok(lean::fit(budget, result.steps.len(), |kept| {
        let mut out = format!("{} to {}  {}{inferred}\n", result.from, result.to, result.direction);
        out.push_str(&lean::symbol_lines(
            &result.steps[..kept],
            Detail::One,
            false,
            &mut HashSet::new(),
        ));
        out.push_str(&code_tail(
            result.omitted + (result.steps.len() - kept),
            result.note.as_deref(),
        ));
        out
    }))
}

fn render_explain(result: &Explain, json: bool, budget: Option<usize>) -> anyhow::Result<String> {
    if json {
        return pretty(result);
    }
    // The budget cuts key symbols first and files after them, as the engine orders them.
    let keys = result.key_symbols.len();
    let total = keys + result.files.len();
    Ok(lean::fit(budget, total, |kept| {
        let key_symbols = &result.key_symbols[..kept.min(keys)];
        let files = &result.files[..kept.saturating_sub(keys)];
        let mut out = format!("{}\n", result.target);
        if !result.entries.is_empty() {
            out.push_str("Entries:\n");
            for row in &result.entries {
                out.push_str(&lean::entry_line(row));
            }
        }
        if !result.folders.is_empty() {
            out.push_str("Folders:\n");
            for row in &result.folders {
                let noun = if row.files == 1 { "file" } else { "files" };
                out.push_str(&format!("{}/  {} {noun}\n", row.folder, row.files));
            }
        }
        if !result.folder_edges.is_empty() {
            out.push_str("Between folders:\n");
            for edge in &result.folder_edges {
                out.push_str(&format!(
                    "{} -> {}  {} exact, {} inferred\n",
                    edge.from, edge.to, edge.exact, edge.inferred
                ));
            }
        }
        if !result.packages.is_empty() {
            out.push_str(&format!("Packages: {}\n", result.packages.join(", ")));
        }
        if !result.libraries.is_empty() {
            out.push_str("Libraries:\n");
            for lib in &result.libraries {
                let owner = lib.component.as_ref().map_or(String::new(), |c| format!("  ({c})"));
                out.push_str(&format!("{}  {}/{owner}\n", lib.name, lib.folder));
            }
        }
        if !files.is_empty() {
            let (lang, rows) = lean::file_lines(files);
            let lang = if lang.is_empty() { lang } else { format!(" ({lang})") };
            out.push_str(&format!("Files{lang}:\n{rows}"));
        }
        if !key_symbols.is_empty() {
            out.push_str("Key symbols:\n");
            out.push_str(&lean::symbol_lines(key_symbols, Detail::Two, true, &mut HashSet::new()));
        }
        if !result.uses.is_empty() {
            out.push_str("Uses:\n");
            for link in &result.uses {
                out.push_str(&format!("{}: {}\n", link.target, link.description));
            }
        }
        for (title, list) in [("Depends on", &result.outbound), ("Used by", &result.inbound)] {
            if !list.is_empty() {
                out.push_str(&format!("{title}:\n"));
                for dep in list {
                    out.push_str(&format!(
                        "{}  {} exact, {} inferred\n",
                        dep.component, dep.exact, dep.inferred
                    ));
                }
            }
        }
        for (title, list) in [("Tasks", &result.tasks), ("Docs", &result.docs)] {
            if !list.is_empty() {
                out.push_str(&format!("{title}: {}\n", list.join(", ")));
            }
        }
        if out.lines().count() == 1 && total == 0 {
            out.push_str("nothing indexed here: no files, symbols, links, tasks or docs\n");
        }
        out.push_str(&code_tail(result.omitted + (total - kept), result.note.as_deref()));
        out
    }))
}

fn render_entries(result: &Entries, json: bool, budget: Option<usize>) -> anyhow::Result<String> {
    if json {
        return pretty(result);
    }
    let total: usize = result.groups.iter().map(|group| group.entries.len()).sum();
    if total == 0 {
        return Ok(format!(
            "no entry points found\n{}",
            code_tail(0, result.note.as_deref())
        ));
    }
    Ok(lean::fit(budget, total, |kept| {
        let mut left = kept;
        let mut out = String::new();
        for group in &result.groups {
            if left == 0 {
                break;
            }
            out.push_str(&format!("{}\n", group.component));
            for row in group.entries.iter().take(left) {
                out.push_str(&lean::entry_line(row));
            }
            left = left.saturating_sub(group.entries.len());
        }
        out.push_str(&code_tail(result.omitted + (total - kept), result.note.as_deref()));
        out
    }))
}

/// `6.2s` from a second up, else `850ms`.
fn duration(millis: u64) -> String {
    if millis >= 1000 {
        format!("{:.1}s", millis as f64 / 1000.0)
    } else {
        format!("{millis}ms")
    }
}

/// One line per indexer (ran, read, skipped with the reason, failed), then the totals.
fn render_native(report: &NativeReport, json: bool) -> anyhow::Result<String> {
    if json {
        return pretty(report);
    }
    let mut out: String = report.indexers.iter().map(native_line).collect();
    out.push_str(&native_summary(report));
    Ok(out)
}

/// `scip-dotnet  ran  1m 20s`, or the reason it was skipped or failed.
fn native_line(run: &IndexerRun) -> String {
    let mut out = format!("{}  {}", run.indexer, run.status);
    if !run.detail.is_empty() {
        out.push_str(&format!("  {}", run.detail));
    } else if run.millis > 0 {
        out.push_str(&format!("  {}", duration(run.millis)));
    }
    out.push('\n');
    out
}

/// The totals; the breakdown only after a full pass, which counts it.
fn native_summary(report: &NativeReport) -> String {
    match (report.upgraded, report.hidden, report.confirmed) {
        (Some(upgraded), Some(hidden), Some(confirmed)) => format!(
            "{} native edges: {upgraded} guessed now exact, {hidden} wrong guesses hidden, {confirmed} already exact, {}\n",
            report.edges,
            duration(report.millis)
        ),
        _ => format!("{} native edges, {}\n", report.edges, duration(report.millis)),
    }
}

/// Nothing for an item nobody links to.
fn render_backlinks(backlinks: &[Backlink]) -> String {
    if backlinks.is_empty() {
        return String::new();
    }
    let mut out = String::from("\nBacklinks:\n");
    for b in backlinks {
        let anchor = b.anchor.as_ref().map_or(String::new(), |a| format!("#{a}"));
        out.push_str(&format!("{}{anchor}  L{}  {}\n", lean::short_id(&b.id), b.line, b.kind));
    }
    out
}

/// Which verb a write result is printed with.
#[derive(Clone, Copy)]
enum Shown {
    Created,
    Edited,
}

fn parse_assignment(arg: &str) -> Result<(String, String), String> {
    match arg.split_once('=') {
        Some((key, value)) if !key.is_empty() => Ok((key.to_string(), value.to_string())),
        _ => Err(format!("expected key=value, got `{arg}`")),
    }
}

fn read_stdin() -> anyhow::Result<String> {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text)?;
    Ok(text)
}

fn show(written: &Written, shown: Shown, dry_run: bool, json: bool) -> anyhow::Result<()> {
    for warning in &written.warnings {
        eprintln!("warning: {warning}");
    }
    print!("{}", render_written(written, shown, dry_run, json)?);
    Ok(())
}

fn render_written(w: &Written, shown: Shown, dry_run: bool, json: bool) -> anyhow::Result<String> {
    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(w)?));
    }
    Ok(if dry_run {
        with_newline(w.text.clone())
    } else {
        match shown {
            Shown::Created => format!("{}\n", w.id),
            Shown::Edited if w.changed => format!("updated {}\n", w.id),
            Shown::Edited => "no change\n".to_string(),
        }
    })
}

/// What `init` did about the stone block in `CLAUDE.md` or `AGENTS.md`.
enum BlockOffer {
    /// Written (or already current) in this file.
    Added(String),
    /// Not written; this file would hold it.
    Declined(String),
}

/// Asks before the block goes into the user's file: `--agent-block` is the yes, a terminal is asked,
/// and anything else (a pipe, CI, `--json`) is a no, so nothing is written unasked.
fn offer_agent_block(root: &std::path::Path, yes: bool, json: bool) -> anyhow::Result<BlockOffer> {
    let root = std::path::absolute(root)?;
    let file = agent_block::target(&root)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let agreed = yes || (!json && std::io::stdin().is_terminal() && ask_yes(&file)?);
    if !agreed {
        return Ok(BlockOffer::Declined(file));
    }
    let done = Repo::open(&root)?.write_agent_block()?;
    Ok(BlockOffer::Added(done.file))
}

fn ask_yes(file: &str) -> anyhow::Result<bool> {
    eprint!("Add a short stone block to {file} so agents find the stone CLI? Text outside it is never touched. [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}

fn render_agent_block(done: &AgentBlock, json: bool) -> anyhow::Result<String> {
    if json {
        let value = serde_json::json!({ "file": done.file, "changed": done.changed });
        return Ok(format!("{}\n", serde_json::to_string_pretty(&value)?));
    }
    Ok(if done.changed {
        format!("updated the stone block in {}\n", done.file)
    } else {
        format!("no change: the stone block in {} is current\n", done.file)
    })
}

fn render_init(done: &Initialised, block: &BlockOffer, json: bool) -> anyhow::Result<String> {
    if json {
        let (stone_repo, skipped) = match &done.stone_repo {
            StoneRepo::NotNeeded => ("not-needed", None),
            StoneRepo::Created => ("created", None),
            StoneRepo::Skipped(why) => ("skipped", Some(why)),
        };
        let value = serde_json::json!({
            "config": done.config.display().to_string(),
            "code_roots": done.code_roots,
            "stone_repo": stone_repo,
            "stone_repo_skipped": skipped,
            "ignored": done.ignored,
            "langs": done.langs,
            "agent_block": match block {
                BlockOffer::Added(file) => serde_json::json!({ "state": "added", "file": file }),
                BlockOffer::Declined(file) => serde_json::json!({ "state": "not-added", "file": file }),
            },
        });
        return Ok(format!("{}\n", serde_json::to_string_pretty(&value)?));
    }
    let mut out = format!("initialised {}\n", done.config.display());
    if done.code_roots == ["."] {
        out.push_str("code roots: the whole repo\n");
    } else if !done.code_roots.is_empty() {
        let n = done.code_roots.len();
        out.push_str(&format!(
            "code roots: {n} project repo{}\n",
            if n == 1 { "" } else { "s" }
        ));
    }
    match &done.stone_repo {
        StoneRepo::NotNeeded => {}
        StoneRepo::Created => out.push_str("stone/ is a git repo of its own\n"),
        StoneRepo::Skipped(why) => out.push_str(&format!(
            "git was not run in stone/ ({why}); install git and run `git init` there\n"
        )),
    }
    if !done.ignored.is_empty() {
        out.push_str(&format!("added to .gitignore: {}\n", done.ignored.join(", ")));
    }
    out.push_str(&next_steps(&done.langs));
    match block {
        BlockOffer::Added(file) => out.push_str(&format!("stone block added to {file}\n")),
        BlockOffer::Declined(file) => out.push_str(&format!(
            "stone block not added; `stone instructions --write` adds it to {file} so agents find the CLI\n"
        )),
    }
    Ok(out)
}

/// What to run after `init`, built from the languages it found.
fn next_steps(langs: &[LangSetup]) -> String {
    if langs.is_empty() {
        return "no code found under the code roots\n\
                next:\n  \
                stone new task --title \"...\"  adds the first task\n  \
                stone serve --open            opens the project in the browser\n"
            .to_string();
    }
    let found: Vec<String> = langs
        .iter()
        .map(|lang| {
            let noun = if lang.files == 1 { "file" } else { "files" };
            format!("{} ({} {noun})", lang.label, lang.files)
        })
        .collect();
    let mut out = format!(
        "languages: {}\n\
         next:\n  \
         stone check         builds the code index; the first run reads every file and can take minutes\n  \
         stone scan          drafts components for the code no component owns\n  \
         stone serve --open  opens the project in the browser\n\
         exact code edges (optional; then run `stone code native`):\n",
        found.join(", ")
    );
    for lang in langs {
        if lang.installed {
            out.push_str(&format!("  {}: {} is installed\n", lang.label, lang.indexer));
        } else {
            out.push_str(&format!("  {}: {}\n", lang.label, lang.install));
        }
    }
    out
}

/// One line per task with the day written, then the total.
fn render_filled(filled: &[Written], dry_run: bool, json: bool) -> anyhow::Result<String> {
    let days: Vec<(&str, &str)> = filled
        .iter()
        .map(|w| {
            let day = w
                .text
                .lines()
                .find_map(|line| line.strip_prefix("closed: "))
                .unwrap_or("");
            (w.id.as_str(), day)
        })
        .collect();
    let count = days.len();
    let noun = if count == 1 { "task" } else { "tasks" };
    let total = if dry_run {
        format!("{count} {noun} would be filled")
    } else {
        format!("{count} {noun} filled")
    };
    if json {
        let rows: Vec<_> = days
            .iter()
            .map(|(id, day)| serde_json::json!({ "id": id, "closed": day }))
            .collect();
        return pretty(&serde_json::json!({ "filled": rows, "count": count, "dry_run": dry_run }));
    }
    let mut out = String::new();
    for (id, day) in days {
        out.push_str(&format!("{id}  closed {day}\n"));
    }
    out.push_str(&total);
    out.push('\n');
    Ok(out)
}

fn render_removed(r: &Removed, dry_run: bool, json: bool) -> anyhow::Result<String> {
    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(r)?));
    }
    let verb = if dry_run { "would remove" } else { "removed" };
    let mut out = format!("{verb} {} ({})\n", r.id, r.path);
    if !r.broken.is_empty() {
        let (left, names) = if dry_run {
            ("would leave", r.broken.join(", "))
        } else {
            ("left", r.broken.join(", "))
        };
        out.push_str(&format!("{left} broken links in: {names}\n"));
    }
    Ok(out)
}

fn render_renamed(r: &Renamed, dry_run: bool, json: bool) -> anyhow::Result<String> {
    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(r)?));
    }
    if !r.changed {
        return Ok(format!("no change: {} is already {}\n", r.from, r.to));
    }
    let (rename, rewrite) = if dry_run {
        ("would rename", "would rewrite")
    } else {
        ("renamed", "rewrote")
    };
    let mut out = format!("{rename} {} -> {}\n", r.from, r.to);
    for path in &r.rewritten {
        out.push_str(&format!("{rewrite} {path}\n"));
    }
    if !r.review.is_empty() {
        out.push_str("review, links not rewritten:\n");
        for path in &r.review {
            out.push_str(&format!("  {path}\n"));
        }
    }
    Ok(out)
}

fn render_renumbered(r: &Renumbered, dry_run: bool, json: bool) -> anyhow::Result<String> {
    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(r)?));
    }
    let (renumber, rewrite) = if dry_run {
        ("would renumber", "would rewrite")
    } else {
        ("renumbered", "rewrote")
    };
    let mut out = format!("{renumber} {} -> {}\n", r.from, r.to);
    for path in &r.rewritten {
        out.push_str(&format!("{rewrite} {path}\n"));
    }
    if !r.review.is_empty() {
        out.push_str("review, links not rewritten:\n");
        for path in &r.review {
            out.push_str(&format!("  {path}\n"));
        }
    }
    Ok(out)
}
