#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ReportFormat {
    Json,
    Text,
}
