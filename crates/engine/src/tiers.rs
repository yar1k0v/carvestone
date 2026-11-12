//! Output tiers: one rule for the size of an answer.
//!
//! Every printed part of an answer is either critical or optional. A critical part
//! prints whole whatever the budget; an optional part shrinks to fit. The cut is by
//! whole lines, a heading prints only when at least one of its lines prints, and a
//! cut answer ends with one line that names what it left out and how large it is.
//!
//! With a budget of `b`, an answer that can be cut costs at most `max(b, tokens of
//! the critical parts plus that last line)`.

use serde::Serialize;

/// Tokens as `Hit::cost` counts them: chars / 4, rounded up.
pub fn tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

/// Whether a part is printed whole or may shrink to fit the budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Critical,
    Optional,
}

/// One named block of an answer: its printed lines and the heading they sit under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Part {
    pub name: String,
    pub tier: Tier,
    pub heading: Option<String>,
    pub lines: Vec<String>,
}

impl Part {
    pub fn critical(name: &str, lines: Vec<String>) -> Part {
        Part {
            name: name.to_string(),
            tier: Tier::Critical,
            heading: None,
            lines,
        }
    }

    pub fn optional(name: &str, heading: Option<&str>, lines: Vec<String>) -> Part {
        Part {
            name: name.to_string(),
            tier: Tier::Optional,
            heading: heading.map(str::to_string),
            lines,
        }
    }
}

/// How much one part lost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LeftOut {
    pub name: String,
    pub lines: usize,
    pub tokens: usize,
}

/// The size of `dropped`, the printed lines an answer left out; `None` when nothing was dropped.
pub fn left_out(name: &str, dropped: &[String]) -> Option<LeftOut> {
    (!dropped.is_empty()).then(|| LeftOut {
        name: name.to_string(),
        lines: dropped.len(),
        tokens: tokens(&dropped.join("\n")),
    })
}

/// The line that ends an answer which cut `left` off. `more_uncounted` says that more was cut
/// that nobody counted; `raise` names the flags that bring it back.
pub fn left_out_line(left: &LeftOut, more_uncounted: bool, raise: &str) -> String {
    let noun = if left.lines == 1 { "line" } else { "lines" };
    let more = if more_uncounted { " and more not counted" } else { "" };
    format!(
        "left out: {} ({} {noun}, about {} tokens){more}; raise {raise}",
        left.name, left.lines, left.tokens
    )
}

/// The answer and what it left out of each part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fitted {
    pub text: String,
    pub left_out: Vec<LeftOut>,
}

/// Fits every part into `budget * 4` characters, the measure [`tokens`] uses.
pub fn fit(parts: &[Part], budget: Option<usize>) -> Fitted {
    let whole: Vec<usize> = parts.iter().map(|part| part.lines.len()).collect();
    let Some(limit) = budget.map(|budget| budget.saturating_mul(4)) else {
        return Fitted {
            text: render(parts, &whole, None),
            left_out: Vec::new(),
        };
    };
    let total: usize = parts.iter().map(part_cost).sum();
    if total <= limit {
        return Fitted {
            text: render(parts, &whole, None),
            left_out: Vec::new(),
        };
    }

    let critical: usize = parts
        .iter()
        .filter(|part| part.tier == Tier::Critical)
        .map(part_cost)
        .sum();
    let optional: Vec<&Part> = parts
        .iter()
        .filter(|part| part.tier == Tier::Optional && !part.lines.is_empty())
        .collect();
    let names: Vec<&str> = optional.iter().map(|part| part.name.as_str()).collect();
    let dropped_lines: usize = optional.iter().map(|part| part.lines.len()).sum();
    let dropped_tokens: usize = optional.iter().map(|part| tokens(&part.lines.join("\n"))).sum();
    let long = cost(&notice(&names, dropped_lines, dropped_tokens));
    let short = cost(&notice(&[], dropped_lines, dropped_tokens));
    let named = critical + long <= limit;
    let reserved = if named { long } else { short };
    let mut room = limit.saturating_sub(critical + reserved);

    let mut order: Vec<usize> = parts
        .iter()
        .enumerate()
        .filter(|(_, part)| part.tier == Tier::Optional && !part.lines.is_empty())
        .map(|(at, _)| at)
        .collect();
    order.sort_by_key(|&at| (part_cost(&parts[at]), at));
    let mut kept = whole.clone();
    for (handled, &at) in order.iter().enumerate() {
        let part = &parts[at];
        let share = room / (order.len() - handled);
        let heading = part.heading.as_deref().map_or(0, cost);
        let mut spend = heading;
        let mut lines = 0;
        for line in &part.lines {
            let step = cost(line);
            if spend + step > share {
                break;
            }
            spend += step;
            lines += 1;
        }
        if lines == 0 {
            spend = 0;
        }
        kept[at] = lines;
        room -= spend;
    }

    let left_out: Vec<LeftOut> = parts
        .iter()
        .zip(&kept)
        .filter(|(part, keep)| **keep < part.lines.len())
        .map(|(part, keep)| {
            let dropped = &part.lines[*keep..];
            LeftOut {
                name: part.name.clone(),
                lines: dropped.len(),
                tokens: tokens(&dropped.join("\n")),
            }
        })
        .collect();
    let cut_names: Vec<&str> = left_out.iter().map(|out| out.name.as_str()).collect();
    let cut_lines: usize = left_out.iter().map(|out| out.lines).sum();
    let cut_tokens: usize = left_out.iter().map(|out| out.tokens).sum();
    let line = notice(if named { &cut_names } else { &[] }, cut_lines, cut_tokens);
    let text = if left_out.is_empty() {
        render(parts, &kept, None)
    } else {
        render(parts, &kept, Some(&line))
    };
    Fitted { text, left_out }
}

/// One printed line's cost: its characters plus the newline that ends it.
fn cost(line: &str) -> usize {
    line.chars().count() + 1
}

/// A part's whole cost, heading included only when a line of it prints.
fn part_cost(part: &Part) -> usize {
    if part.lines.is_empty() {
        return 0;
    }
    part.heading.as_deref().map_or(0, cost) + part.lines.iter().map(|line| cost(line)).sum::<usize>()
}

/// The line that ends a cut answer, naming the cut parts when `names` is not empty.
fn notice(names: &[&str], lines: usize, tokens: usize) -> String {
    let noun = if lines == 1 { "line" } else { "lines" };
    if names.is_empty() {
        format!("left out: {lines} {noun}, about {tokens} tokens; raise --budget")
    } else {
        format!(
            "left out: {} ({lines} {noun}, about {tokens} tokens); raise --budget",
            names.join(", ")
        )
    }
}

/// Prints each part to its kept line count, then the notice line.
fn render(parts: &[Part], kept: &[usize], notice: Option<&str>) -> String {
    let mut text = String::new();
    for (part, keep) in parts.iter().zip(kept) {
        if *keep == 0 {
            continue;
        }
        if let Some(heading) = &part.heading {
            text.push_str(heading);
            text.push('\n');
        }
        for line in &part.lines[..*keep] {
            text.push_str(line);
            text.push('\n');
        }
    }
    if let Some(notice) = notice {
        text.push_str(notice);
        text.push('\n');
    }
    text
}
