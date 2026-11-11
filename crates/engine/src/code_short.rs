//! The short form of a declaration's signature, for an outline read at a glance: modifiers and
//! keywords such as `public`, `async` or `function` dropped, one word per parameter, long generic
//! arguments folded to `…`, and the return type after a `:` in every language.
//!
//! It reads the signature text the extractors stored, so it never needs the source again. Text it
//! does not recognise comes back folded and capped rather than refused.

/// Words that say how a declaration is reached or run, never what it is.
const MODIFIERS: &[&str] = &[
    "public",
    "private",
    "protected",
    "internal",
    "static",
    "async",
    "sealed",
    "override",
    "virtual",
    "abstract",
    "readonly",
    "partial",
    "export",
    "default",
    "const",
    "let",
    "var",
    "function",
    "def",
    "fn",
    "pub",
    "pub(crate)",
    "pub(super)",
    "pub(self)",
    "unsafe",
    "extern",
    "new",
    "required",
    "file",
    "declare",
    "volatile",
    "implicit",
    "explicit",
    "operator",
    "get",
    "set",
    "*",
    // Dart.
    "final",
    "late",
    "covariant",
    "external",
    "factory",
];

/// Words that say what a type or module is; the short form keeps them.
const KINDS: &[&str] = &[
    "class",
    "record",
    "struct",
    "interface",
    "enum",
    "trait",
    "impl",
    "type",
    "namespace",
    "mod",
    "module",
    "delegate",
    "event",
    "union",
];

/// Parameter words that are no part of its type.
const PARAM_MODIFIERS: &[&str] = &[
    "this",
    "ref",
    "out",
    "in",
    "params",
    "readonly",
    "scoped",
    "mut",
    "required",
    "covariant",
    "final",
];

const MAX: usize = 100;
const MAX_PARAMS: usize = 4;
const MAX_ARGS: usize = 24;

/// The short form of `signature`, a declaration named `name` of kind `kind` (`type`, `module`,
/// `function`, `method` or `member`).
pub fn short_signature(signature: &str, name: &str, kind: &str) -> String {
    let signature = signature.trim();
    if signature.is_empty() || name.is_empty() {
        return name.to_string();
    }
    // A Dart class's unnamed constructor is `new` (as in `Foo.new`), written under the class name.
    let name = if name == "new" && find_word(signature, name).is_none() {
        class_name_before_params(signature).unwrap_or(name)
    } else {
        name
    };
    let Some(at) = find_word(signature, name) else {
        return cap(fold(signature));
    };
    let prefix = &signature[..at];
    let mut rest = signature[at + name.len()..].trim_start();

    let mut keyword = None;
    let mut return_type = Vec::new();
    for word in top_level_words(prefix) {
        if MODIFIERS.contains(&word) || word.starts_with("pub(") {
            continue;
        }
        // `factory Foo.make()` and `void IFoo.Bar()`: the owner in front of the name is no type.
        if word.ends_with('.') {
            continue;
        }
        if KINDS.contains(&word) {
            keyword.get_or_insert(word);
            continue;
        }
        return_type.push(word);
    }

    let mut generics = String::new();
    if rest.starts_with('<')
        && let Some(close) = matching(rest, 0)
    {
        generics = fold(&rest[..=close]);
        rest = rest[close + 1..].trim_start();
    }
    // `const f = async (x) => ...` and `{ f: async (x) => ... }`: the value is the function.
    let assigned = rest.strip_prefix('=').filter(|v| !v.starts_with('='));
    if let Some(value) = assigned.or_else(|| rest.strip_prefix(':')) {
        let value = value.trim_start();
        let value = value.strip_prefix("async").map_or(value, str::trim_start);
        let mut value = value.strip_prefix("function").map_or(value, str::trim_start);
        // `get: <T>(path) => ...`: a generic function value.
        if value.starts_with('<')
            && let Some(close) = matching(value, 0)
            && value[close + 1..].trim_start().starts_with('(')
        {
            generics = fold(&value[..=close]);
            value = value[close + 1..].trim_start();
        }
        if value.starts_with('(') {
            rest = value.trim_end().trim_end_matches("=>").trim_end();
        } else if assigned.is_some() {
            rest = "";
        }
    }

    let mut params = None;
    if rest.starts_with('(')
        && let Some(close) = matching(rest, 0)
    {
        params = Some(short_params(&rest[1..close]));
        rest = rest[close + 1..].trim_start();
    }
    let tail = trim_tail(rest);

    let mut out = String::new();
    let is_type = matches!(kind, "type" | "module");
    // `type type Query`: the outline's label already says it.
    if is_type
        && let Some(keyword) = keyword
        && keyword != kind
    {
        out.push_str(keyword);
        out.push(' ');
    }
    out.push_str(name);
    out.push_str(&generics);
    if let Some(params) = &params {
        out.push('(');
        out.push_str(params);
        out.push(')');
    }
    let joined = return_type.join(" ");
    let after = if is_type {
        tail
    } else if let Some(tail) = tail.and_then(|t| t.strip_prefix(':').or_else(|| t.strip_prefix("->"))) {
        Some(tail.trim())
    } else if !joined.is_empty() {
        Some(joined.as_str())
    } else {
        None
    };
    let Some(after) = after.filter(|a| !a.is_empty() && *a != "void") else {
        return cap(out);
    };
    // The return type or bases as written while the line stays short, folded once it does not.
    let line = |after: &str| {
        let mut line = out.clone();
        if is_type && !after.starts_with(':') {
            line.push(' ');
            line.push_str(after);
        } else {
            // A field reads `name: type`; a function `name(params) : return`.
            line.push_str(if kind == "member" { ": " } else { " : " });
            line.push_str(after.trim_start_matches(':').trim());
        }
        line
    };
    let full = line(after);
    if full.chars().count() <= MAX {
        return full;
    }
    cap(line(&fold(after)))
}

/// The last word before the first `(` when it is a plain identifier: `Foo` of `const Foo({..})`.
fn class_name_before_params(signature: &str) -> Option<&str> {
    let head = &signature[..signature.find('(')?];
    let word = head.split_whitespace().last()?;
    word.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        .then_some(word)
}

/// The bases or return type after the parameters, without a `where` clause, a value or a body.
fn trim_tail(rest: &str) -> Option<&str> {
    let mut end = rest.len();
    for stop in [" where ", " = ", "=>", " {"] {
        if let Some(found) = find_top_level(rest, stop) {
            end = end.min(found);
        }
    }
    let tail = rest[..end].trim().trim_end_matches([';', ',', '=']).trim();
    (!tail.is_empty()).then_some(tail)
}

/// One word per parameter, its type in every language (its name where it has none); `self`,
/// attributes and defaults dropped; more than a few elided.
fn short_params(params: &str) -> String {
    let mut words = Vec::new();
    collect_params(params, &mut words);
    if words.len() > MAX_PARAMS {
        words.truncate(MAX_PARAMS - 1);
        words.push("…".to_string());
    }
    words.join(", ")
}

/// The whole of `param` when it is one bracketed group, without its brackets: Dart writes its
/// named parameters `{required String a, int b = 0}` and its optional ones `[int c]`.
fn group_inner(param: &str) -> Option<&str> {
    let closes = match param.chars().next()? {
        '{' => '}',
        '[' => ']',
        _ => return None,
    };
    (param.ends_with(closes) && matching(param, 0) == Some(param.len() - 1)).then(|| &param[1..param.len() - 1])
}

fn collect_params(params: &str, words: &mut Vec<String>) {
    for param in split_top_level(params, ',') {
        let param = match find_top_level(param, "=") {
            Some(at) if !param[at..].starts_with("=>") => &param[..at],
            _ => param,
        };
        if let Some(inner) = group_inner(param.trim()) {
            collect_params(inner, words);
            continue;
        }
        // `[FromQuery] string search`, `@Body() dto: Dto`: an attribute is no part of the parameter.
        let mut param = param.trim();
        while let Some(word) = top_level_words(param).first().copied()
            && (word.starts_with('[') || word.starts_with('@'))
        {
            param = param[word.len()..].trim_start();
        }
        if param.is_empty() {
            continue;
        }
        let word = if let Some(colon) = find_top_level(param, ":") {
            // `data: InstallationFormData` reads as its type, like a C# parameter.
            let ty = param[colon + 1..].trim();
            if ty.is_empty() {
                param[..colon]
                    .trim()
                    .trim_end_matches('?')
                    .trim_start_matches("...")
                    .to_string()
            } else {
                fold(ty)
            }
        } else {
            let parts: Vec<&str> = top_level_words(param)
                .into_iter()
                .filter(|w| !PARAM_MODIFIERS.contains(w))
                .collect();
            match parts.as_slice() {
                [] => continue,
                [one] => (*one).to_string(),
                [types @ .., _name] => fold(&types.join(" ")),
            }
        };
        // A Dart initialising formal `this.storage` names the field it sets.
        let word = match word.strip_prefix("this.").or_else(|| word.strip_prefix("super.")) {
            Some(field) => field.to_string(),
            None => word,
        };
        let bare = word.trim_start_matches('&').trim_start_matches("mut ");
        if matches!(bare, "self" | "cls" | "this") {
            continue;
        }
        words.push(word);
    }
}

/// One level of generic arguments is kept, deeper ones become `…`: `Task<PagedResult<…>>`.
/// Arguments still longer than a few words become `…` too.
fn fold(text: &str) -> String {
    fold_at(text.trim(), 0)
}

fn fold_at(text: &str, level: usize) -> String {
    let mut out = String::new();
    let mut at = 0;
    while let Some(offset) = text[at..].find('<') {
        let open = at + offset;
        out.push_str(&text[at..=open]);
        let Some(close) = matching(text, open) else {
            out.push_str(&text[open + 1..]);
            return out;
        };
        let inner = fold_at(&text[open + 1..close], level + 1);
        if level > 0 || inner.chars().count() > MAX_ARGS {
            out.push('…');
        } else {
            out.push_str(&inner);
        }
        out.push('>');
        at = close + 1;
    }
    out.push_str(&text[at..]);
    out
}

fn cap(text: String) -> String {
    if text.chars().count() <= MAX {
        return text;
    }
    let mut out: String = text.chars().take(MAX - 1).collect();
    out.push('…');
    out
}

/// The byte index of the bracket that closes the one at `open`.
fn matching(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let bytes = text.as_bytes();
    for (index, &byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'<' | b'(' | b'[' | b'{' => depth += 1,
            b'>' if index > 0 && matches!(bytes[index - 1], b'=' | b'-') => {}
            b'>' | b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// The bracket depth before each byte, with `=>` and `->` no bracket.
fn depths(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut out = Vec::with_capacity(bytes.len());
    for (index, &byte) in bytes.iter().enumerate() {
        out.push(depth);
        match byte {
            b'<' | b'(' | b'[' | b'{' => depth += 1,
            b'>' if index > 0 && matches!(bytes[index - 1], b'=' | b'-') => {}
            b'>' | b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    out
}

fn find_top_level(text: &str, needle: &str) -> Option<usize> {
    let depth = depths(text);
    text.match_indices(needle).map(|(at, _)| at).find(|&at| depth[at] == 0)
}

fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let depth = depths(text);
    let mut parts = Vec::new();
    let mut start = 0;
    for (at, ch) in text.char_indices() {
        if ch == separator && depth[at] == 0 {
            parts.push(&text[start..at]);
            start = at + ch.len_utf8();
        }
    }
    parts.push(&text[start..]);
    parts
}

fn top_level_words(text: &str) -> Vec<&str> {
    split_top_level(text, ' ')
        .into_iter()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect()
}

/// The first place `word` stands as a whole identifier.
fn find_word(text: &str, word: &str) -> Option<usize> {
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    text.match_indices(word).map(|(at, _)| at).find(|&at| {
        let before = text[..at].chars().next_back().is_none_or(|c| !ident(c));
        let after = text[at + word.len()..].chars().next().is_none_or(|c| !ident(c));
        before && after
    })
}
