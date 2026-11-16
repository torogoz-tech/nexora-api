use crate::{report_format::ReportFormat, unknown_mode::UnknownMode};
use std::path::PathBuf;
#[derive(clap::Args)]
pub struct GovernanceCheckArguments {
    #[arg(conflicts_with = "snapshot")]
    pub path: Option<PathBuf>,
    #[arg(long, requires = "repository")]
    pub snapshot: Option<PathBuf>,
    #[arg(long)]
    pub export_snapshot: Option<PathBuf>,
    #[arg(long)]
    pub repository: Option<String>,
    #[arg(long)]
    pub policies: PathBuf,
    #[arg(long, default_value = "local")]
    pub tenant: String,
    #[arg(long)]
    pub organization: Option<String>,
    #[arg(long, default_value = ".nexora/governance")]
    pub store: PathBuf,
    #[arg(long, value_enum, default_value = "json")]
    pub format: ReportFormat,
    #[arg(long, value_enum, default_value = "policy")]
    pub unknown: UnknownMode,
}
