//! Local MCP boundary over immutable persistent Nexora knowledge.
mod snapshot_query;
mod tool_catalog;
pub use snapshot_query::SnapshotQuery;
pub use tool_catalog::ToolCatalog;
mod frame_buffer;
mod mcp_configuration;
mod mcp_output;
mod mcp_service;
mod protocol_session;
mod request_registry;
mod stdio_server;
pub use mcp_configuration::McpConfiguration;
pub use stdio_server::StdioServer;
#[cfg(test)]
mod output_tests;
#[cfg(test)]
mod session_tests;

mod knowledge_tool_catalog;
