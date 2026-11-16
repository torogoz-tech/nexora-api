use clap::Subcommand;
use std::path::PathBuf;
#[derive(Subcommand)]
pub enum LanguageCommand {
    List,
    ExportProfile {
        catalog: PathBuf,
        language: String,
        key: String,
        version: String,
        name: String,
        output: PathBuf,
    },
    Validate {
        path: PathBuf,
    },
}
