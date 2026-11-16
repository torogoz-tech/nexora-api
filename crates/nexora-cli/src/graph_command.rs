use clap::Subcommand;
#[derive(Subcommand)]
pub enum GraphCommand {
    /// Traverse persisted Neo4j identities in a published snapshot.
    Traverse {
        #[arg(long, default_value = "local")]
        tenant: String,
        #[arg(long)]
        repository: String,
        #[arg(long)]
        snapshot: String,
        #[arg(long)]
        node: String,
        #[arg(long,default_value_t=3,value_parser=clap::value_parser!(u8).range(0..=32))]
        depth: u8,
        #[arg(long,default_value_t=100,value_parser=clap::value_parser!(u16).range(1..=1000))]
        limit: u16,
    },
}
