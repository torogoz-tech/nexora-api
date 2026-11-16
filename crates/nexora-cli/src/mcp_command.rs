use clap::Subcommand;
use std::path::PathBuf;
#[derive(Subcommand)]
pub enum McpCommand {
    /// Serve MCP over stdio for one configured local repository and tenant.
    Serve {
        #[arg(long)]
        root: PathBuf,
        #[arg(long, default_value = "local")]
        tenant: String,
        /// Expose indexing of the configured root; disabled by default.
        #[arg(long)]
        allow_index: bool,
        /// Allow contribution writes separately from indexing; actor is local provenance, not authentication.
        #[arg(long, requires = "knowledge_actor")]
        allow_knowledge: bool,
        #[arg(long, requires = "allow_knowledge")]
        knowledge_actor: Option<String>,
    },
}
