use crate::report_format::ReportFormat;
use std::path::PathBuf;
#[derive(clap::Args)]
pub struct GovernanceReportArguments {
    #[arg(long, requires = "repository")]
    pub evaluation: Option<String>,
    #[arg(long)]
    pub repository: Option<String>,
    #[arg(long)]
    pub snapshot_id: Option<String>,
    #[arg(long, default_value = "local")]
    pub tenant: String,
    #[arg(long, default_value = ".nexora/governance")]
    pub store: PathBuf,
    #[arg(long, value_enum, default_value = "json")]
    pub format: ReportFormat,
}
