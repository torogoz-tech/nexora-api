use clap::Parser;
use std::path::PathBuf;
#[derive(Parser)]
#[command(about = "Evidence-grounded OpenAI reasoning over an authenticated Nexora repository")]
pub struct Arguments {
    #[arg(long)]
    pub openai_key_file: PathBuf,
    #[arg(long)]
    pub nexora_key_file: PathBuf,
    #[arg(long)]
    pub request: PathBuf,
    #[arg(long)]
    pub project: String,
    #[arg(long)]
    pub repository: String,
    #[arg(long)]
    pub snapshot: Option<String>,
    #[arg(long)]
    pub baseline_snapshot: Option<String>,
    #[arg(long, default_value_t = 18080)]
    pub port: u16,
    #[arg(long, default_value = "gpt-6.1-sol")]
    pub model: String,
    #[arg(long, default_value = "medium")]
    pub effort: String,
    /// Validate local repository scope without sending anything to OpenAI.
    #[arg(long)]
    pub check_scope: bool,
}
