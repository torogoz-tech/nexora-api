use clap::Subcommand;
use std::{net::SocketAddr, path::PathBuf};
#[derive(Subcommand)]
pub enum ApiCommand {
    /// Attest an existing local snapshot without changing its runtime identity or data.
    AdoptLocal(crate::LocalSnapshotArguments),
    /// Retire trust in an imported local source.
    RevokeLocal(crate::LocalSourceRevocationArguments),
    /// Explicitly establish a new trust epoch for a revoked local source.
    ReauthorizeLocal(crate::LocalSnapshotArguments),
    /// Produce credential protocol headers for a local MCP client, without database access.
    McpHeaders {
        /// Private API-key file; never pass a key value as an argument.
        #[arg(long)]
        api_key_file: PathBuf,
    },
    /// Process registered repository synchronization jobs without executing repository code.
    Worker {
        /// Operator-owned profiles and private working directory.
        #[arg(long)]
        settings: PathBuf,
        #[arg(long)]
        git_policy: Option<PathBuf>,
        /// Process at most one job and exit; otherwise keep consuming the queue.
        #[arg(long)]
        once: bool,
    },
    /// Apply versioned platform migrations; does not modify engine snapshots.
    Migrate,
    /// Initialize this installation once, without predefined credentials.
    Bootstrap {
        #[arg(long)]
        organization: String,
        #[arg(long)]
        username: String,
        #[arg(long)]
        name: String,
        /// Existing private file; its contents are never printed.
        #[arg(long)]
        password_file: PathBuf,
    },
    /// Serve the local authenticated HTTP API.
    Serve {
        /// Exact HTTPS frontend origin; repeat for additional authorized browser origins.
        #[arg(long)]
        cors_origin: Vec<String>,
        /// Explicit operator configuration for native OpenAI reasoning.
        #[arg(long)]
        reasoning_settings: Option<PathBuf>,
        #[arg(long, default_value = "127.0.0.1:18080")]
        bind: SocketAddr,
        #[arg(long, default_value_t = 4)]
        workers: usize,
        /// Operator-owned origin allowlist for corporate OIDC; contains no client secrets.
        #[arg(long)]
        identity_policy: Option<PathBuf>,
        #[arg(long)]
        git_policy: Option<PathBuf>,
    },
}
