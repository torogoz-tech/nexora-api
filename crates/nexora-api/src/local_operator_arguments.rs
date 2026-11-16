use clap::{ArgGroup, Args};
use std::path::PathBuf;

#[derive(Args)]
#[command(group(ArgGroup::new("local_operator").required(true).args(["session_file","username"])))]
pub struct LocalOperatorArguments {
    /// Private human-session file, including sessions authenticated through corporate OIDC.
    #[arg(long, conflicts_with_all = ["username", "password_file"])]
    pub session_file: Option<PathBuf>,
    /// Local login, only when permitted by this installation's authentication policy.
    #[arg(long, requires = "password_file")]
    pub username: Option<String>,
    #[arg(long, requires = "username")]
    pub password_file: Option<PathBuf>,
}
