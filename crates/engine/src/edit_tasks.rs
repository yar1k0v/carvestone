//! `stone link`, `stone arch connect`, `stone task move`, `stone task next` and `stone task fill-closed`.

use std::collections::HashSet;

use carvestone_core::{ItemType, USES_MAX_WORDS, sections, uses_bullets};
use carvestone_store::Entry;

use crate::arch_graph::target_id;
use crate::create::{Written, stamp_by, stamp_value, today};
use crate::edit::type_of;
use crate::fields::{SetCtx, apply_set, check_done_sections, check_vocab, sync_closed};
use crate::query::{ListFilter, ListRow};
use crate::repo::{EngineError, Repo};
use crate::work_links::{WorkLinks, tasks};

impl Repo {
    pub fn link(&self, from: &str, to: &[String], dry_run: bool) -> Result<Written, EngineError> {
        if to.is_empty() {
            return Err(EngineError::Invalid("nothing to link; pass at least one target".into()));
        }
        self.update(from, dry_run, |snapshot, entry, doc| {
            let ctx = SetCtx {
                snapshot,
                config: &self.store.config,
                ty: type_of(entry)?,
            };
            for target in to {
                apply_set(&ctx, doc, "links", &format!("+{target}"))?;
            }
            Ok(())
        })
    }

    /// Writes one connection from `from` to `to`: `to` joins `uses`, and `## Uses` gets the bullet
    /// `- [[to]]: does`, with ` (over X)` when the link runs over a protocol. A bullet for `to`
    /// that is already there is replaced; every other line stays, so the same call twice changes nothing.
    pub fn connect(
        &self,
        from: &str,
        to: &str,
        does: &str,
        over: Option<&str>,
        dry_run: bool,
    ) -> Result<Written, EngineError> {
        let does = does.split_whitespace().collect::<Vec<_>>().join(" ");
        let words = does.split(' ').filter(|w| !w.is_empty()).count();
        if words == 0 {
            return Err(EngineError::Invalid(
                "`--does` is empty; say what the connection does, such as `Saves items`".into(),
            ));
        }
        if words > USES_MAX_WORDS {
            return Err(EngineError::Invalid(format!(
                "`--does` has {words} words; shorten it to a verb phrase of {USES_MAX_WORDS} or fewer, such as `Saves items`"
            )));
        }
        let over = over.map(|o| o.split_whitespace().collect::<Vec<_>>().join(" "));
        if let Some(over) = &over {
            if over.is_empty() {
                return Err(EngineError::Invalid(
                    "`--over` is empty; name the protocol, such as `HTTP`, or leave `--over` out".into(),
                ));
            }
            if over.contains("[[") {
                return Err(EngineError::Invalid(format!(
                    "`--over` holds `[[`; write the protocol name alone, such as `NATS`, because a link has to resolve (`{over}`)"
                )));
            }
            if over.contains(['(', ')']) {
                return Err(EngineError::Invalid(format!(
                    "`--over {over}` holds a parenthesis; write the protocol name alone, such as `NATS`"
                )));
            }
        }
        if does.contains("[[") {
            return Err(EngineError::Invalid(
                "`--does` holds `[[`; write plain words, because a link in a description has to resolve".into(),
            ));
        }
        // A trailing parenthesis in `does` reads as a protocol, and a bullet that reads back
        // different from what was asked for is worse than a refusal.
        let probe = uses_bullets(&format!("## Uses\n{}\n", uses_bullet("x", &does, over.as_deref())), 1);
        if !matches!(probe.as_slice(), [read] if read.description == does && read.technology.as_deref() == over.as_deref())
        {
            return Err(EngineError::Invalid(format!(
                "`--does {does}` would not read back as written; remove a trailing parenthesis from it, and put a protocol in `--over`"
            )));
        }
        let outcome = self.update(from, dry_run, |snapshot, entry, doc| {
            let parts = |entry: &Entry| matches!(type_of(entry), Ok(ItemType::System | ItemType::Container | ItemType::Component));
            if !parts(entry) {
                return Err(EngineError::Invalid(format!(
                    "{} is not a system, container or component; connect two parts of the design",
                    entry.path
                )));
            }
            let (target, anchor) = snapshot.resolve(to).map_err(not_in_store)?;
            if anchor.is_some() || !parts(target) {
                return Err(EngineError::Invalid(format!(
                    "`{to}` is not a system, container or component; name the part this one connects to, such as `stone arch hld` lists"
                )));
            }
            if target.id == entry.id {
                return Err(EngineError::Invalid(format!(
                    "{} cannot connect to itself; name a different part",
                    entry.path
                )));
            }
            let label = snapshot.label(target);
            let ctx = SetCtx {
                snapshot,
                config: &self.store.config,
                ty: type_of(entry)?,
            };
            let listed = apply_set(&ctx, doc, "uses", &format!("+{label}"))?;
            let before = doc.body.clone();

            let bullet = uses_bullet(&label, &does, over.as_deref());
            let bullets = uses_bullets(&doc.body, 1);
            let held = bullets
                .iter()
                .find(|b| target_id(snapshot, &b.target) == target.id.as_str());
            doc.body = match held {
                Some(held) => replace_bullet(&doc.body, held.line, &bullet),
                None => add_bullet(&doc.body, &bullet, bullets.last().map(|b| b.line), doc.eol()),
            };
            // Stamped like `stone edit` does, and only when something changed, so a repeat writes nothing.
            if listed || doc.body != before {
                doc.set("generated", stamp_value(&stamp_by(self)?, &today()))?;
            }
            Ok(())
        });
        // The target's own miss is already worded in the closure; what is left here is `from`.
        outcome.map_err(not_in_store)
    }

    pub fn move_task(&self, input: &str, status: &str, dry_run: bool) -> Result<Written, EngineError> {
        self.update(input, dry_run, |snapshot, entry, doc| {
            let ty = type_of(entry)?;
            if ty == ItemType::Plan {
                let plan = snapshot.label(entry);
                return Err(EngineError::Invalid(format!(
                    "{plan} is a plan, and plans carry no status; create a task with `stone new task <title>` \
                     and link it to the plan with `stone link <task> {plan}`"
                )));
            }
            if ty != ItemType::Task {
                return Err(EngineError::NotATask {
                    id: snapshot.label(entry),
                });
            }
            check_vocab(&self.store.config, ty, "status", status)?;
            if status == "done" {
                check_done_sections(&self.store.config, &snapshot.label(entry), &doc.body, doc.body_line())?;
            }
            let ctx = SetCtx {
                snapshot,
                config: &self.store.config,
                ty,
            };
            let was = doc.get_str("status");
            apply_set(&ctx, doc, "status", status)?;
            sync_closed(&ctx, doc, was.as_deref())
        })
    }

    /// Writes `closed` into every done task that has none, from its newest commit or else the last
    /// change of its file, so a date no longer has to be guessed on every read. Archived tasks and
    /// tasks that already have a day stay as they are, so a second run writes nothing.
    pub fn fill_closed(&self, dry_run: bool) -> Result<Vec<Written>, EngineError> {
        let snapshot = self.snapshot()?;
        let links = WorkLinks::load(self, &snapshot)?;
        let wanted: Vec<(String, String)> = tasks(&snapshot)
            .filter(|(_, item)| item.fields.status.as_deref() == Some("done") && item.fields.closed.is_none())
            .filter_map(|(entry, item)| Some((snapshot.label(entry), links.closed(entry, item)?.day)))
            .collect();
        let mut written = Vec::new();
        for (id, day) in wanted {
            // Each write re-reads under the lock, so a task closed meanwhile is left alone.
            let done = self.update(&id, dry_run, |snapshot, entry, doc| {
                if doc.get_str("status").as_deref() != Some("done") || doc.get_str("closed").is_some() {
                    return Ok(());
                }
                let ctx = SetCtx {
                    snapshot,
                    config: &self.store.config,
                    ty: type_of(entry)?,
                };
                apply_set(&ctx, doc, "closed", &day)?;
                Ok(())
            })?;
            if done.changed {
                written.push(done);
            }
        }
        Ok(written)
    }

    pub fn next(&self, limit: usize) -> Result<Vec<ListRow>, EngineError> {
        let snapshot = self.snapshot()?;
        let ready: HashSet<&str> = snapshot
            .entries
            .iter()
            .filter_map(|entry| {
                let item = entry.item.as_ref()?;
                if item.item_type() != Some(ItemType::Task) {
                    return None;
                }
                // A dependency that does not resolve counts as not done.
                let all_done = item.fields.depends_on.iter().all(|dep| {
                    snapshot.resolve(dep).is_ok_and(|(found, _)| {
                        found
                            .item
                            .as_ref()
                            .is_some_and(|d| d.fields.status.as_deref() == Some("done"))
                    })
                });
                all_done.then_some(entry.id.as_str())
            })
            .collect();
        let mut rows: Vec<ListRow> = self
            .list(&ListFilter {
                item_type: Some("task".into()),
                status: Some("todo".into()),
                ..ListFilter::default()
            })?
            .into_iter()
            .filter(|row| ready.contains(row.id.as_str()))
            .collect();
        // Stable, so the number order from `list` holds inside one priority.
        let priorities = &self.store.config.priorities;
        rows.sort_by_key(|row| {
            row.priority
                .as_ref()
                .and_then(|p| priorities.iter().position(|q| q == p))
                .unwrap_or(usize::MAX)
        });
        rows.truncate(limit);
        Ok(rows)
    }
}

/// `- [[target]]: does`, with ` (over X)` when the link runs over a protocol.
fn uses_bullet(target: &str, does: &str, over: Option<&str>) -> String {
    match over {
        Some(over) => format!("- [[{target}]]: {does} (over {over})"),
        None => format!("- [[{target}]]: {does}"),
    }
}

/// An item that is not in the store, worded with the next step.
fn not_in_store(err: EngineError) -> EngineError {
    match err {
        EngineError::NotFound { id, suggestion } => EngineError::Invalid(format!(
            "`{id}` is not in the store; name an existing system, container or component (`stone arch hld` lists them){}",
            suggestion.map(|s| format!(". Did you mean {s}?")).unwrap_or_default()
        )),
        other => other,
    }
}

/// True for a line that continues the bullet above it: indented, not blank, and not the start of
/// another list item at any indent.
fn continues_bullet(line: &str) -> bool {
    let text = line.trim_start();
    line.starts_with([' ', '\t'])
        && !text.is_empty()
        && !["- ", "* ", "+ "].iter().any(|marker| text.starts_with(marker))
}

/// The index after line `idx` (from 0) and the indented lines under it that are not list items.
fn bullet_end(lines: &[&str], idx: usize) -> usize {
    let under = lines[idx + 1..].iter().take_while(|l| continues_bullet(l)).count();
    idx + 1 + under
}

/// The body with the bullet at line `line` (from 1) and the indented lines under it that are not
/// list items swapped for `text`; the bullet's indent, its line ending and every other line stay.
fn replace_bullet(body: &str, line: usize, text: &str) -> String {
    let lines: Vec<&str> = body.split_inclusive('\n').collect();
    let idx = line - 1;
    let ending = if lines[idx].ends_with("\r\n") {
        "\r\n"
    } else if lines[idx].ends_with('\n') {
        "\n"
    } else {
        ""
    };
    let indent = &lines[idx][..lines[idx].len() - lines[idx].trim_start().len()];
    let mut out = lines[..idx].concat();
    out.push_str(indent);
    out.push_str(text);
    out.push_str(ending);
    out.push_str(&lines[bullet_end(&lines, idx)..].concat());
    out
}

/// The body with `bullet` straight after the last bullet of `## Uses` (`last`, a line from 1), or
/// at the top of the section when it has none, or in a new section at the end. No other line
/// changes; `eol` is the file's line ending.
fn add_bullet(body: &str, bullet: &str, last: Option<usize>, eol: &str) -> String {
    let lines: Vec<&str> = body.split_inclusive('\n').collect();
    let blank = |line: Option<&&str>| line.is_some_and(|l| l.trim().is_empty());
    // (index to insert at, text to insert)
    let (at, insert) = if let Some(last) = last {
        (bullet_end(&lines, last - 1), format!("{bullet}{eol}"))
    } else if let Some(section) = sections(body, 1)
        .into_iter()
        .find(|s| s.title.eq_ignore_ascii_case("uses"))
    {
        // `start_line` is the heading, so as an index it is the line after it.
        let after = section.start_line;
        let (at, lead) = if blank(lines.get(after)) {
            (after + 1, "")
        } else {
            (after, eol)
        };
        let trail = if lines.get(at).is_some_and(|l| !l.trim().is_empty()) {
            eol
        } else {
            ""
        };
        (at, format!("{lead}{bullet}{eol}{trail}"))
    } else {
        let mut out = body.to_string();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(eol);
        }
        if !out.is_empty() && !blank(lines.last()) {
            out.push_str(eol);
        }
        out.push_str(&format!("## Uses{eol}{eol}{bullet}{eol}"));
        return out;
    };
    let mut out = lines[..at].concat();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(eol);
    }
    out.push_str(&insert);
    out.push_str(&lines[at..].concat());
    out
}
