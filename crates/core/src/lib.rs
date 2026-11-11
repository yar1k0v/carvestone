//! carvestone-core: the domain model shared by every interface (CLI, MCP, HTTP).
//!
//! Nothing here touches the filesystem or the index; `carvestone-store` and
//! `carvestone-index` depend on this crate, never the other way round.

pub mod error;
pub mod id;
pub mod item;
pub mod section;

pub use error::{Error, Result};
pub use id::{IdScheme, ItemId, ItemType, NumberedKind, Ref};
pub use item::{
    Actor, CONTAINER_KINDS, Fields, Item, LEVELS, LIST_KEYS, SCALAR_KEYS, SYSTEM_KINDS, Stamp, Trust, trust,
};
pub use section::{
    C4Box, C4Rel, Checkbox, Diagram, Fact, Link, Section, USES_MAX_WORDS, UsesBullet, body_links, c4_parts, check_box,
    checkboxes, code_span_ranges, diagrams, doc_facts, first_paragraph, lines, links, md_links, plain_text,
    section_text, sections, set_section, slug, uses_bullets,
};
