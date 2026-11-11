//! The MCP face of the engine: a thin adapter that parses tool arguments and
//! renders engine results as text. All behaviour lives in `carvestone-engine`.
//!
//! It is opt-in and read-only, for clients that cannot run a shell. Agents with
//! a shell use the `stone` CLI, which also writes.
#![deny(clippy::print_stdout)]

use carvestone_engine::Repo;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as ProtocolError, RoleServer, ServerHandler, ServiceExt};

pub mod errors;
mod tools;

pub use errors::McpError;

/// What a client learns at initialize: the tools are reads, writes need the CLI.
const INSTRUCTIONS: &str = "Read-only view of the stone store: search, get, list and code. \
Changing the store (new, edit, link, task move) is done with the `stone` CLI.";

/// The server: one open repo.
pub struct StoneServer {
    repo: Repo,
}

impl StoneServer {
    pub fn new(repo: Repo) -> Self {
        Self { repo }
    }
}

impl ServerHandler for StoneServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(rmcp::model::Implementation::new("stone", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ProtocolError> {
        Ok(ListToolsResult::with_all_items(tools::definitions()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ProtocolError> {
        let args = request.arguments.unwrap_or_default();
        let result = errors::catch_panic(|| tools::call(request.name.as_ref(), &args, &self.repo))
            .unwrap_or_else(errors::tool_error);
        Ok(result.into())
    }
}

/// Serves the engine on stdin and stdout until the client closes the stream.
pub async fn serve_stdio(repo: Repo) -> Result<(), McpError> {
    StoneServer::new(repo)
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}

/// The same server over an in-process stream, so tests drive the real protocol.
#[doc(hidden)]
pub async fn serve_stream(repo: Repo, stream: tokio::io::DuplexStream) -> Result<(), McpError> {
    StoneServer::new(repo).serve(stream).await?.waiting().await?;
    Ok(())
}
