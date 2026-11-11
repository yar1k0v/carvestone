//! Bodies the HTTP server sends that no other engine call returns: write replies, the error body
//! and the event payloads. They live here so one export covers every type the web UI reads.

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::fields::SetValue;

/// A field value as the UI sends it: text, a number, a flag, or a list that replaces a list field.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(untagged)]
pub enum FieldValue {
    Text(String),
    Number(serde_json::Number),
    Flag(bool),
    List(Vec<String>),
}

impl FieldValue {
    /// The change this value asks for: text read as `--set` syntax, or a list that replaces the field.
    pub fn into_value(self) -> SetValue {
        match self {
            FieldValue::Text(text) => SetValue::Text(text),
            FieldValue::Number(number) => SetValue::Text(number.to_string()),
            FieldValue::Flag(flag) => SetValue::Text(flag.to_string()),
            FieldValue::List(items) => SetValue::List(items),
        }
    }
}

/// A JSON object read as ordered pairs, so `{"a": 1, "b": 2}` is applied in the order written.
#[derive(Debug, Clone, PartialEq)]
pub struct Pairs<T>(pub Vec<(String, T)>);

impl<T> Default for Pairs<T> {
    fn default() -> Self {
        Pairs(Vec::new())
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Pairs<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PairsVisitor<T>(std::marker::PhantomData<T>);

        impl<'de, T: Deserialize<'de>> Visitor<'de> for PairsVisitor<T> {
            type Value = Pairs<T>;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Pairs<T>, A::Error> {
                let mut pairs = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, T>()? {
                    if pairs.iter().any(|(seen, _): &(String, T)| *seen == key) {
                        return Err(de::Error::custom(format!("duplicate key `{key}`")));
                    }
                    pairs.push((key, value));
                }
                Ok(Pairs(pairs))
            }
        }

        deserializer.deserialize_map(PairsVisitor(std::marker::PhantomData))
    }
}

/// A section to replace: its heading text and the new text under it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SectionEdit {
    pub name: String,
    pub text: String,
}

/// The body of `PATCH /api/items/{id}`: what `stone edit` takes.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct PatchRequest {
    #[serde(default)]
    #[cfg_attr(
        feature = "ts",
        ts(as = "Option<std::collections::BTreeMap<String, FieldValue>>", optional)
    )]
    pub set: Pairs<FieldValue>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<String>>", optional))]
    pub unset: Vec<String>,
    #[cfg_attr(feature = "ts", ts(optional))]
    pub section: Option<SectionEdit>,
    /// Replace the section although it holds sub-sections the new text does not carry.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub replace_nested: bool,
    /// The 1-based checkbox of `## Acceptance criteria` to tick.
    #[cfg_attr(feature = "ts", ts(optional))]
    pub check_ac: Option<usize>,
    /// A line to add under `## Notes`, dated today; a send-back comment goes here.
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub dry_run: bool,
    /// The etag the client last read; a different one on disk refuses the write.
    #[cfg_attr(feature = "ts", ts(optional))]
    pub if_match: Option<String>,
}

/// The body of `POST /api/items`: what `stone new` takes.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CreateRequest {
    #[serde(rename = "type")]
    pub item_type: String,
    pub title: String,
    #[cfg_attr(feature = "ts", ts(optional))]
    pub slug: Option<String>,
    #[serde(default)]
    #[cfg_attr(
        feature = "ts",
        ts(as = "Option<std::collections::BTreeMap<String, FieldValue>>", optional)
    )]
    pub fields: Pairs<FieldValue>,
    /// Section name to text, written in the order given.
    #[serde(default)]
    #[cfg_attr(
        feature = "ts",
        ts(as = "Option<std::collections::BTreeMap<String, String>>", optional)
    )]
    pub sections: Pairs<String>,
}

/// The JSON body of every failed request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ErrorBody {
    /// A plain sentence that names the next step.
    pub error: String,
    /// `not_found`, `invalid`, `conflict`, `bad_request`, `forbidden` or `internal`.
    pub kind: String,
}

/// The reply of `PATCH /api/items/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PatchReply {
    pub id: String,
    pub changed: bool,
    /// Of the file on disk after the call.
    pub etag: String,
    /// The resulting file; only for a dry run.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub text: Option<String>,
}

/// The reply of `POST /api/items`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CreateReply {
    pub id: String,
    pub path: String,
    pub etag: String,
}

/// `event: items`: stone items whose files changed on disk, as path ids and short ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ItemsEvent {
    pub ids: Vec<String>,
}

/// `event: code`: code files under the code roots that changed on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CodeEvent {
    pub paths: Vec<String>,
}

/// `event: error`: the refresh failed; the next batch tries again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ErrorEvent {
    pub error: String,
}
