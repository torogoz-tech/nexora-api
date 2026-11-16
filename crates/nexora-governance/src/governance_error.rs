#[derive(Debug, thiserror::Error)]
pub enum GovernanceError {
    #[error("policy configuration: {0}")]
    Configuration(String),
    #[error("governance resource limit: {0}")]
    Limit(String),
    #[error("governance scope: {0}")]
    Scope(String),
    #[error("governance I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("governance serialization: {0}")]
    Json(#[from] serde_json::Error),
}
