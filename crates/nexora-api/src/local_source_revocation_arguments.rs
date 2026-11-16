use crate::LocalOperatorArguments;
use clap::Args;

#[derive(Args)]
pub struct LocalSourceRevocationArguments {
    #[arg(long)]
    pub project: String,
    #[arg(long)]
    pub repository: String,
    #[arg(long)]
    pub expected_revision: i64,
    #[arg(long)]
    pub reason: String,
    #[arg(long)]
    pub request_id: String,
    #[command(flatten)]
    pub operator: LocalOperatorArguments,
}
