use clap::Subcommand;
#[derive(Subcommand)]
pub enum StorageCommand {
    /// Verify real PostgreSQL, Dragonfly and Neo4j connections.
    Check,
    /// Prepare selective indexes for an already published snapshot without changing current.
    PrepareQueries {
        #[arg(long, default_value = "local")]
        tenant: String,
        #[arg(long)]
        repository: String,
        #[arg(long)]
        snapshot: Option<String>,
    },
}
