use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid id `{0}`: {1}")]
    InvalidId(String, &'static str),

    #[error("invalid actor `{0}`: {1}")]
    InvalidActor(String, &'static str),

    #[error("{id} not found{}", suggestion.as_ref().map(|s| format!(". Did you mean {s}?")).unwrap_or_default())]
    NotFound { id: String, suggestion: Option<String> },

    #[error("schema error in {id}: {message}")]
    Schema { id: String, message: String },

    #[error("no section `{0}`")]
    SectionNotFound(String),

    #[error("section `{section}` has {count} checkboxes, no. {n} does not exist")]
    CheckboxOutOfRange { section: String, n: usize, count: usize },
}

pub type Result<T> = std::result::Result<T, Error>;
