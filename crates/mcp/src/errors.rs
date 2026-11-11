//! Failures of the MCP face: one error for the transport, plus the panic guard
//! every handler body runs through.
//!
//! Engine errors pass through unchanged: they are already plain sentences that
//! name the next step, and the `✗` prefix belongs to the CLI alone.

use carvestone_engine::EngineError;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::service::ServerInitializeError;

/// A failure to start or run the server, as opposed to a failed tool call.
#[derive(Debug)]
pub enum McpError {
    /// An engine call failed before or while the transport ran.
    Engine(EngineError),
    /// The transport could not start or the handshake failed.
    Protocol(String),
    /// The serving task panicked or was cancelled.
    Task(String),
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpError::Engine(err) => write!(f, "{err}"),
            McpError::Protocol(text) => write!(f, "{text}"),
            McpError::Task(text) => write!(f, "{text}"),
        }
    }
}

impl std::error::Error for McpError {}

impl From<EngineError> for McpError {
    fn from(err: EngineError) -> Self {
        McpError::Engine(err)
    }
}

impl From<ServerInitializeError> for McpError {
    fn from(err: ServerInitializeError) -> Self {
        McpError::Protocol(err.to_string())
    }
}

impl From<tokio::task::JoinError> for McpError {
    fn from(err: tokio::task::JoinError) -> Self {
        McpError::Task(err.to_string())
    }
}

/// The sentence an unexpected failure gets: a panic must not kill the session.
pub const INTERNAL: &str = "internal error; run `stone check` and retry";

/// Runs a handler body; a panic becomes [`INTERNAL`] instead of ending the server.
pub fn catch_panic<T>(body: impl FnOnce() -> T) -> Result<T, &'static str> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).map_err(|_| INTERNAL)
}

/// A tool result carrying `text` as its single content block.
pub fn text(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(text.into())])
}

/// A tool result the client reads as a failed call, with the reason as its text.
pub fn tool_error(text: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(text.into())])
}
