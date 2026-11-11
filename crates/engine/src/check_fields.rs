//! Field-value rules of `stone check`: vocabularies, stamps, dates, outcomes.

use carvestone_core::{
    Actor, CONTAINER_KINDS, ItemType, LEVELS, SYSTEM_KINDS, USES_MAX_WORDS, section_text, sections, uses_bullets,
};
use carvestone_store::Config;

use crate::arch_graph::target_id;
use crate::check::Problem;
use crate::resolve::Snapshot;

pub(crate) fn field_problems(config: &Config, snapshot: &Snapshot, out: &mut Vec<Problem>) {
    for entry in &snapshot.entries {
        let (Ok(doc), Some(item)) = (&entry.doc, &entry.item) else {
            continue;
        };
        let Some(ty) = item.item_type() else { continue };
        let path = entry.path.as_str();
        let line_of = |key: &str| doc.key_line(key).unwrap_or(1);
        let fields = &item.fields;

        if let Some(s) = &fields.status {
            let allowed = config.statuses_for(ty);
            if !allowed.contains(s) {
                out.push(Problem::new(
                    path,
                    line_of("status"),
                    "status",
                    format!(
                        "status `{s}` is not valid for a {}; use one of: {}",
                        item.type_name.as_deref().unwrap_or_default(),
                        allowed.join(", ")
                    ),
                ));
            }
        }
        if let Some(level) = &fields.level
            && !LEVELS.contains(&level.as_str())
        {
            out.push(Problem::new(
                path,
                line_of("level"),
                "level",
                format!("level `{level}` is not valid; use one of: {}", LEVELS.join(", ")),
            ));
        }
        if matches!(ty, ItemType::System | ItemType::Container | ItemType::Component) {
            arch_kind_problems(snapshot, path, ty, fields, &line_of, out);
        }
        if ty == ItemType::Task {
            let vocab = [
                ("kind", "kind", &fields.kind, &config.task_kinds),
                ("priority", "priority", &fields.priority, &config.priorities),
            ];
            for (key, rule, value, allowed) in vocab {
                if let Some(v) = value
                    && !allowed.contains(v)
                {
                    out.push(Problem::new(
                        path,
                        line_of(key),
                        rule,
                        format!(
                            "{key} `{v}` is not valid for a task; use one of: {}",
                            allowed.join(", ")
                        ),
                    ));
                }
            }
        }
        if let Some(created) = &fields.created
            && !is_date(created)
        {
            out.push(Problem::new(
                path,
                line_of("created"),
                "date",
                format!("created `{created}` is not a date; use YYYY-MM-DD"),
            ));
        }
        if let Some(stale_after) = &fields.stale_after
            && !is_date(stale_after)
        {
            out.push(Problem::new(
                path,
                line_of("stale_after"),
                "date",
                format!("stale_after `{stale_after}` is not a date; use YYYY-MM-DD"),
            ));
        }
        for key in ["generated", "verified"] {
            let line = line_of(key);
            match doc.get_stamps(key) {
                Err(e) => out.push(Problem::new(path, line, "stamp", e.to_string())),
                Ok(stamps) => {
                    for stamp in stamps {
                        if let Err(e) = stamp.by.parse::<Actor>() {
                            out.push(Problem::new(path, line, "actor", e.to_string()));
                        }
                        if !is_date(&stamp.at) {
                            out.push(Problem::new(
                                path,
                                line,
                                "date",
                                format!("{key} `at` value `{}` is not a date; use YYYY-MM-DD", stamp.at),
                            ));
                        }
                    }
                }
            }
        }
        if ty == ItemType::Task && fields.status.as_deref() == Some("done") {
            let has_outcome = sections(&item.body, item.body_line)
                .iter()
                .find(|s| s.title.eq_ignore_ascii_case("Outcome"))
                .is_some_and(|s| {
                    section_text(&item.body, item.body_line, s)
                        .lines()
                        .skip(1)
                        .any(|l| !l.trim().is_empty())
                });
            if !has_outcome {
                let id = snapshot.label(entry);
                out.push(Problem::new(
                    path,
                    line_of("status"),
                    "outcome",
                    format!("{id} has no ## Outcome; add one with `stone edit {id} --section outcome`"),
                ));
            }
        }
    }
}

/// The `kind` and `parent` rules of arch items: a kind fits the item's type, a system with a kind sits
/// in no product, and only a system with no kind is a parent.
fn arch_kind_problems(
    snapshot: &Snapshot,
    path: &str,
    ty: ItemType,
    fields: &carvestone_core::Fields,
    line_of: &dyn Fn(&str) -> usize,
    out: &mut Vec<Problem>,
) {
    if let Some(kind) = &fields.kind {
        let (name, allowed): (&str, &[&str]) = match ty {
            ItemType::System => ("system", &SYSTEM_KINDS),
            ItemType::Container => ("container", &CONTAINER_KINDS),
            _ => ("component", &[]),
        };
        if !allowed.contains(&kind.as_str()) {
            let tail = if allowed.is_empty() {
                "a component takes no kind".to_string()
            } else {
                format!("use one of: {}", allowed.join(", "))
            };
            out.push(Problem::new(
                path,
                line_of("kind"),
                "kind",
                format!("kind `{kind}` is not valid for a {name}; {tail}"),
            ));
        }
    }
    let Some(parent) = &fields.parent else { return };
    if ty == ItemType::System
        && let Some(kind) = &fields.kind
        && SYSTEM_KINDS.contains(&kind.as_str())
    {
        out.push(Problem::new(
            path,
            line_of("parent"),
            "parent",
            format!("a system with kind `{kind}` cannot have a parent; remove `parent`"),
        ));
    }
    // A parent that does not resolve is the link rule's to report.
    let Ok((found, _)) = snapshot.resolve(parent) else {
        return;
    };
    let Some(parent_item) = &found.item else { return };
    let parent_ty = parent_item.item_type();
    if ty == ItemType::System && parent_ty != Some(ItemType::System) {
        out.push(Problem::new(
            path,
            line_of("parent"),
            "parent",
            format!("parent `{parent}` is not a system; a system's parent must be a system"),
        ));
    }
    if parent_ty == Some(ItemType::System)
        && let Some(kind) = &parent_item.fields.kind
    {
        out.push(Problem::new(
            path,
            line_of("parent"),
            "parent",
            format!("parent `{parent}` is a system with kind `{kind}`; only a system with no kind can be a parent"),
        ));
    }
}

/// Warnings for the `## Uses` section of arch items: a bullet whose target `uses` does not list, and a
/// description over `USES_MAX_WORDS` words. A description with several reasons is judged part by part.
pub(crate) fn uses_problems(snapshot: &Snapshot, out: &mut Vec<Problem>) {
    for entry in &snapshot.entries {
        let Some(item) = &entry.item else { continue };
        if !crate::arch_graph::is_arch(item.item_type()) {
            continue;
        }
        let listed: Vec<String> = item.fields.uses.iter().map(|v| target_id(snapshot, v)).collect();
        for bullet in uses_bullets(&item.body, item.body_line) {
            if snapshot.resolve(&bullet.target).is_err() {
                continue; // `body-link` already reports it.
            }
            let id = target_id(snapshot, &bullet.target);
            if !listed.contains(&id) {
                out.push(Problem::warning(
                    &entry.path,
                    bullet.line,
                    "uses-link",
                    format!(
                        "`## Uses` names {} but `uses` does not list it; add it to `uses` or remove the bullet",
                        bullet.target
                    ),
                ));
            }
            let longest = bullet
                .description
                .split(';')
                .map(|part| part.split_whitespace().count())
                .max()
                .unwrap_or(0);
            if longest > USES_MAX_WORDS {
                out.push(Problem::warning(
                    &entry.path,
                    bullet.line,
                    "uses-words",
                    format!(
                        "the description of {} in `## Uses` has {longest} words; shorten it to a verb phrase of {USES_MAX_WORDS} or fewer, such as `Saves items`",
                        bullet.target
                    ),
                ));
            }
        }
    }
}

/// `YYYY-MM-DD`, optionally followed by `T` and anything (a timestamp).
pub(crate) fn is_date(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() < 10 || (b.len() > 10 && b[10] != b'T') {
        return false;
    }
    let digits = |r: std::ops::Range<usize>| -> Option<u32> {
        let part = &b[r];
        if part.iter().all(u8::is_ascii_digit) {
            Some(part.iter().fold(0u32, |acc, d| acc * 10 + u32::from(d - b'0')))
        } else {
            None
        }
    };
    if b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let (Some(year), Some(month), Some(day)) = (digits(0..4), digits(5..7), digits(8..10)) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}
