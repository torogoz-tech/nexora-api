use crate::LocalOperatorArguments;
use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct LocalSnapshotArguments {
    #[arg(long)]
    pub project: String,
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub root: PathBuf,
    #[arg(long)]
    pub runtime_tenant: String,
    #[arg(long)]
    pub snapshot: String,
    #[arg(long)]
    pub expected_hash: String,
    #[arg(long)]
    pub reason: String,
    #[arg(long)]
    pub request_id: String,
    #[arg(long, requires = "expected_revision")]
    pub repository: Option<String>,
    #[arg(long, requires = "repository")]
    pub expected_revision: Option<i64>,
    #[command(flatten)]
    pub operator: LocalOperatorArguments,
}
