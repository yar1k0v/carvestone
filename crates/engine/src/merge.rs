//! Three-way merge of a stone item: front matter by field, body by paragraph.
//! Git calls it through `stone merge-file`, so only edits to the same paragraph
//! or the same field stay a conflict.

use std::ops::Range;

use carvestone_core::section::{blocks, eol};
use carvestone_store::frontmatter::Document;
use serde_norway::Value;
use similar::{Algorithm, DiffOp, capture_diff_slices};

use crate::repo::EngineError;

/// The merged file text and how many conflict blocks it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    pub text: String,
    pub conflicts: usize,
}

/// Merges `ours` and `theirs` against their common ancestor `base`.
pub fn merge_text(base: &str, ours: &str, theirs: &str) -> Merged {
    let (Ok(b), Ok(o), Ok(t)) = (Document::parse(base), Document::parse(ours), Document::parse(theirs)) else {
        return whole_file(base, ours, theirs);
    };
    let line_end = o.eol();
    let (front, front_clash) = merge_front(&b, o.clone(), &t);
    let (body, body_conflicts) = merge_body(&b.body, &o.body, &t.body, line_end);
    let (text, front_conflicts) = if front_clash {
        (
            format!("{}{body}", block(&front_only(&o), &front_only(&t), line_end)),
            1,
        )
    } else {
        let mut doc = front;
        doc.body = body;
        (doc.to_text(), 0)
    };
    Merged {
        text,
        conflicts: body_conflicts + front_conflicts,
    }
}

/// A conflict block: each marker on its own line.
fn block(ours: &str, theirs: &str, line_end: &str) -> String {
    let own_line = |s: &str| {
        if s.is_empty() || s.ends_with('\n') {
            ""
        } else {
            line_end
        }
        .to_string()
    };
    format!(
        "<<<<<<< ours{line_end}{ours}{}======={line_end}{theirs}{}>>>>>>> theirs{line_end}",
        own_line(ours),
        own_line(theirs)
    )
}

/// The `---` block alone, ending in a line break.
fn front_only(doc: &Document) -> String {
    let mut doc = doc.clone();
    doc.body.clear();
    let mut text = doc.to_text();
    if !text.ends_with('\n') {
        text.push_str(doc.eol());
    }
    text
}

/// A side whose front matter does not parse: a side that did not change gives way, else the
/// whole file is one conflict.
fn whole_file(base: &str, ours: &str, theirs: &str) -> Merged {
    let keep = |text: &str| Merged {
        text: text.to_string(),
        conflicts: 0,
    };
    if ours == theirs || base == theirs {
        return keep(ours);
    }
    if base == ours {
        return keep(theirs);
    }
    Merged {
        text: block(ours, theirs, eol(ours)),
        conflicts: 1,
    }
}

/// Merges the fields into a copy of ours, so untouched lines keep their bytes. The flag is
/// true when both sides changed one field to different values.
fn merge_front(base: &Document, mut ours: Document, theirs: &Document) -> (Document, bool) {
    let (fb, fo, ft) = (base.fields(), ours.fields().clone(), theirs.fields());
    let mut keys: Vec<&Value> = fo.keys().collect();
    keys.extend(ft.keys().filter(|k| !fo.contains_key(*k)));
    for key in keys {
        let Value::String(name) = key else {
            continue;
        };
        let (vb, vo, vt) = (fb.get(key), fo.get(key), ft.get(key));
        let want = if vo == vt || vt == vb || name == "generated" {
            vo.cloned()
        } else if vo == vb {
            vt.cloned()
        } else if let Some(list) = union_lists(vb, vo, vt) {
            Some(list)
        } else {
            return (ours, true);
        };
        let done = match want {
            Some(v) => ours.set(name, v),
            None => ours.remove(name).map(|_| ()),
        };
        if done.is_err() {
            return (ours, true);
        }
    }
    (ours, false)
}

/// Both sides changed a list: ours, then what theirs added, minus what either side dropped.
fn union_lists(base: Option<&Value>, ours: Option<&Value>, theirs: Option<&Value>) -> Option<Value> {
    let items = |v: Option<&Value>| match v {
        Some(Value::Sequence(s)) => Some(s.clone()),
        None => Some(Vec::new()),
        _ => None,
    };
    let (base, ours, theirs) = (items(base)?, items(ours)?, items(theirs)?);
    let mut out = ours.clone();
    for item in &theirs {
        if !out.contains(item) {
            out.push(item.clone());
        }
    }
    out.retain(|item| !base.contains(item) || (ours.contains(item) && theirs.contains(item)));
    Some(Value::Sequence(out))
}

/// One side's change: the base units `range` become `units`.
struct Hunk<'a> {
    range: Range<usize>,
    units: Vec<&'a str>,
}

/// Units compare without their trailing blank lines, so a blank line added or lost at the
/// end of a unit is not a change of its text.
fn hunks<'a>(base: &[&str], side: &[&'a str]) -> Vec<Hunk<'a>> {
    let keys = |units: &[&str]| units.iter().map(|u| u.trim_end().to_string()).collect::<Vec<_>>();
    capture_diff_slices(Algorithm::Myers, &keys(base), &keys(side))
        .into_iter()
        .filter(|op| !matches!(op, DiffOp::Equal { .. }))
        .map(|op| Hunk {
            range: op.old_range(),
            units: side[op.new_range()].to_vec(),
        })
        .collect()
}

/// Two changes clash when their base ranges share a unit. An insertion clashes with a change
/// that starts at its position or runs over it; two insertions at one position do not.
fn clash(a: &Range<usize>, b: &Range<usize>) -> bool {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => false,
        (true, false) => b.start <= a.start && a.start < b.end,
        (false, true) => a.start <= b.start && b.start < a.end,
        (false, false) => a.start < b.end && b.start < a.end,
    }
}

/// The base units of `range` with the side's hunks applied.
fn apply(base: &[&str], range: &Range<usize>, side: &[Hunk<'_>]) -> String {
    let mut out = String::new();
    let mut at = range.start;
    for h in side {
        out.extend(base[at..h.range.start].iter().copied());
        out.extend(h.units.iter().copied());
        at = h.range.end;
    }
    out.extend(base[at..range.end].iter().copied());
    out
}

/// Drops the blank line that `merge_body` added at the end.
fn unpad(text: &mut String) {
    let cut = if text.ends_with("\r\n\r\n") {
        2
    } else if text.ends_with("\n\n") {
        1
    } else {
        0
    };
    text.truncate(text.len() - cut);
}

fn merge_body(base: &str, ours: &str, theirs: &str, line_end: &str) -> (String, usize) {
    // Every unit then ends in a blank line, the last one too, so an appended paragraph does not
    // turn the old last paragraph into a changed one. The pad comes off the result again.
    let pad = [base, ours, theirs].iter().all(|b| b.ends_with('\n'));
    let padded = |b: &str| if pad { format!("{b}{}", eol(b)) } else { b.to_string() };
    let (pb, po, pt) = (padded(base), padded(ours), padded(theirs));
    let (ub, uo, ut) = (blocks(&pb), blocks(&po), blocks(&pt));
    let (ha, hb) = (hunks(&ub, &uo), hunks(&ub, &ut));

    let mut out = String::new();
    let (mut conflicts, mut at, mut i, mut j) = (0, 0, 0, 0);
    let mut ends_in_conflict = false;
    while i < ha.len() || j < hb.len() {
        // The next change by base position, ours first on a tie.
        let ours_next = j >= hb.len() || (i < ha.len() && ha[i].range.start <= hb[j].range.start);
        let (first, other) = if ours_next {
            (&ha[i], hb.get(j))
        } else {
            (&hb[j], ha.get(i))
        };
        out.extend(ub[at..first.range.start].iter().copied());
        ends_in_conflict = false;
        let Some(other) = other.filter(|o| clash(&first.range, &o.range)) else {
            out.extend(first.units.iter().copied());
            at = first.range.end;
            // The same paragraphs inserted at one place by both sides count once.
            let twin =
                other.is_some_and(|o| first.range.is_empty() && o.range == first.range && o.units == first.units);
            if ours_next {
                i += 1;
                j += usize::from(twin);
            } else {
                j += 1;
                i += usize::from(twin);
            }
            continue;
        };
        // Clashing changes: grow the span until no later change of either side reaches into it.
        let mut span = first.range.start.min(other.range.start)..first.range.end.max(other.range.end);
        let (mut ci, mut cj) = (i + 1, j + 1);
        loop {
            let mut grew = false;
            while ci < ha.len() && clash(&ha[ci].range, &span) {
                span.end = span.end.max(ha[ci].range.end);
                ci += 1;
                grew = true;
            }
            while cj < hb.len() && clash(&hb[cj].range, &span) {
                span.end = span.end.max(hb[cj].range.end);
                cj += 1;
                grew = true;
            }
            if !grew {
                break;
            }
        }
        let (mut mine, mut yours) = (apply(&ub, &span, &ha[i..ci]), apply(&ub, &span, &hb[j..cj]));
        if mine == yours {
            out.push_str(&mine);
        } else {
            conflicts += 1;
            let last = ci == ha.len() && cj == hb.len() && span.end == ub.len();
            if pad && last {
                unpad(&mut mine);
                unpad(&mut yours);
            }
            out.push_str(&block(&mine, &yours, line_end));
            ends_in_conflict = last;
        }
        at = span.end;
        (i, j) = (ci, cj);
    }
    out.extend(ub[at..].iter().copied());
    if pad && !ends_in_conflict {
        unpad(&mut out);
    }
    (out, conflicts)
}
