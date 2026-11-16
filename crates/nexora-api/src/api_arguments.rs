use crate::api_command::ApiCommand;
use clap::Parser;
use std::path::PathBuf;
#[derive(Parser)]
#[command(
    name = "nexora-api",
    version,
    about = "Nexora self-hosted platform API"
)]
pub struct ApiArguments {
    #[arg(long, default_value = ".nexora/local/connections.json")]
    pub connections: PathBuf,
    #[arg(long, default_value = "nexora_platform")]
    pub schema: String,
    #[command(subcommand)]
    pub command: ApiCommand,
}
