use crate::{
    governance_check_arguments::GovernanceCheckArguments,
    governance_report_arguments::GovernanceReportArguments,
};
#[derive(clap::Subcommand)]
pub enum GovernanceCommand {
    Check(GovernanceCheckArguments),
    Report(GovernanceReportArguments),
}
