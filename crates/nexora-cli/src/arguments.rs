use crate::command::Command;
use clap::Parser;
use std::path::PathBuf;
#[derive(Parser)]
#[command(name = "nexora", version, about = "Declarative code knowledge engine")]
pub struct Arguments {
    #[arg(long, global = true, default_value = "languages")]
    pub language_dir: PathBuf,
    #[arg(long, global = true)]
    pub language: Option<String>,
    #[arg(long, global = true, default_value = ".nexora/local/connections.json")]
    pub connections: PathBuf,
    #[command(subcommand)]
    pub command: Command,
}
