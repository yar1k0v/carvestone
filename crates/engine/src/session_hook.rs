//! The Claude Code SessionStart hook that prints the project map at the start of each session.
//!
//! The hook lives in the user's `.claude/settings.json`, so the merge keeps every other key and
//! the order of the file, and a second run writes nothing.

use crate::repo::{EngineError, Repo};

/// Where Claude Code reads the hooks of a project.
pub const SETTINGS: &str = ".claude/settings.json";
/// The command the hook runs.
pub const COMMAND: &str = "stone map";

/// `text` with a SessionStart hook that runs [`COMMAND`] added, or unchanged when one is already
/// there. `Err` names the key whose JSON type is wrong.
pub fn apply(text: &str) -> Result<String, String> {
    let mut value: serde_json::Value = if text.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(text).map_err(|e| format!("it is not valid JSON: {e}"))?
    };
    let root = value.as_object_mut().ok_or("the root is not a JSON object")?;
    let hooks = root.entry("hooks").or_insert_with(|| serde_json::json!({}));
    let hooks = hooks.as_object_mut().ok_or("`hooks` is not a JSON object")?;
    let sessions = hooks.entry("SessionStart").or_insert_with(|| serde_json::json!([]));
    let sessions = sessions
        .as_array_mut()
        .ok_or("`hooks.SessionStart` is not a JSON array")?;
    let mut present = false;
    for entry in &*sessions {
        let entry_hooks = entry
            .get("hooks")
            .and_then(serde_json::Value::as_array)
            .ok_or("a `hooks.SessionStart` entry has no `hooks` array")?;
        present |= entry_hooks.iter().any(runs_the_map);
    }
    if present {
        return Ok(text.to_string());
    }
    sessions.push(serde_json::json!({
        "hooks": [{ "type": "command", "command": COMMAND }]
    }));
    let mut out = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    out.push('\n');
    Ok(out)
}

/// Whether one hook entry runs `stone map`.
fn runs_the_map(hook: &serde_json::Value) -> bool {
    hook.get("command")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|command| {
            let command = command.trim();
            command.ends_with("stone map") || command.ends_with("stone.exe map")
        })
}

/// What `write_session_hook` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHook {
    /// The file the hook belongs in.
    pub file: String,
    /// False when the file already ran the map.
    pub changed: bool,
}

impl Repo {
    /// Adds the SessionStart hook to `.claude/settings.json` at the repository root.
    pub fn write_session_hook(&self) -> Result<SessionHook, EngineError> {
        let path = self.store.layout.root.join(SETTINGS);
        let changed = self.rewrite_root_file(&path, SETTINGS, |text| {
            apply(text).map_err(|why| {
                EngineError::Invalid(format!(
                    "{SETTINGS}: {why}; nothing was written. Add a SessionStart hook with the command `{COMMAND}` by hand"
                ))
            })
        })?;
        Ok(SessionHook {
            file: SETTINGS.to_string(),
            changed,
        })
    }
}
