use clap::Subcommand;
#[derive(Subcommand)]
pub enum SnapshotCommand {
    /// Read a published snapshot from PostgreSQL without reindexing sources.
    Get {
        #[arg(long, default_value = "local")]
        tenant: String,
        #[arg(long)]
        repository: String,
        /// Omit to read the current published snapshot.
        #[arg(long)]
        snapshot: Option<String>,
    },
}
