use clap::Subcommand;
use std::path::PathBuf;
#[derive(Subcommand)]
pub enum CorpusCommand {
    Validate {
        path: PathBuf,
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
    Check {
        path: PathBuf,
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        #[arg(long)]
        case: Option<String>,
        #[arg(long)]
        include_facts: bool,
    },
}
