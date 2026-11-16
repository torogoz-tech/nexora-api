use crate::language_command::LanguageCommand;
use clap::Subcommand;
use std::path::PathBuf;
#[derive(Subcommand)]
pub enum Command {
    Mcp {
        #[command(subcommand)]
        command: crate::mcp_command::McpCommand,
    },
    Storage {
        #[command(subcommand)]
        command: crate::storage_command::StorageCommand,
    },
    Snapshot {
        #[command(subcommand)]
        command: crate::snapshot_command::SnapshotCommand,
    },
    Graph {
        #[command(subcommand)]
        command: crate::graph_command::GraphCommand,
    },
    /// Reuse analysis in a bounded in-memory session; does not watch or persist sources.
    IndexSession {
        path: PathBuf,
        #[arg(long, default_value = "local")]
        tenant: String,
        #[arg(long,default_value_t=2,value_parser=clap::value_parser!(u16).range(2..=100))]
        passes: u16,
        #[arg(long,default_value_t=67_108_864,value_parser=clap::value_parser!(u64).range(0..=67_108_864))]
        cache_bytes: u64,
    },
    Corpus {
        #[command(subcommand)]
        command: crate::corpus_command::CorpusCommand,
    },
    Policy {
        #[command(subcommand)]
        command: crate::policy_command::PolicyCommand,
    },
    Governance {
        #[command(subcommand)]
        command: crate::governance_command::GovernanceCommand,
    },
    Languages {
        #[command(subcommand)]
        command: LanguageCommand,
    },
    Analyze {
        path: PathBuf,
    },
    /// Persist a snapshot in PostgreSQL and Neo4j, reusing Dragonfly file analysis.
    Index {
        path: PathBuf,
        /// Run explicitly without database services.
        #[arg(long)]
        offline: bool,
        /// Include cache reuse and operation timings in JSON.
        #[arg(long, conflicts_with = "offline")]
        report: bool,
        #[arg(long, default_value = "local")]
        tenant: String,
    },
}
