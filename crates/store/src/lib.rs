//! carvestone-store: the source of truth on disk.
//!
//! Text files so git gives diffs, line-level merges and review. Own lean
//! schema; OKF field names where free (`type`, `generated`, `verified`,
//! `status`). See decision-6.
//!
//! ```text
//! stone/config.yml
//! stone/tasks/        CS-42.md           id tasks/CS-42
//! stone/milestones/   M-3.md
//! stone/decisions/    ADR-7.md
//! stone/plans/        <slug>.md          id plans/<slug>
//! stone/docs/         <slug>.md          id docs/<slug>
//! stone/refs/         <slug>.md          id refs/<slug>
//! stone/arch/         <path>.md          id arch/<path>
//! stone/files/        file store, never scanned
//! .stone/lock         write lock (gitignored)
//! .stone/index.db     derived index (gitignored)
//! ```

pub mod config;
pub mod frontmatter;
pub mod ids;
pub mod layout;
pub mod lock;
pub mod store;
pub mod write;

pub use config::Config;
pub use frontmatter::{Document, FrontmatterError};
pub use layout::Layout;
pub use store::{Entry, FileStat, Store, StoreError};
