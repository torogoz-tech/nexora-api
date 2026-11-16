use clap::Subcommand;
use std::path::PathBuf;
#[derive(Subcommand)]
pub enum PolicyCommand {
    Validate { path: PathBuf },
    Test { path: PathBuf },
}
