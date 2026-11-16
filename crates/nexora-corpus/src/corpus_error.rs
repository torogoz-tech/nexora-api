#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    #[error("corpus configuration: {0}")]
    Configuration(String),
    #[error("corpus resource limit: {0}")]
    Limit(String),
    #[error("corpus I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("corpus serialization: {0}")]
    Json(#[from] serde_json::Error),
    #[error("corpus engine: {0}")]
    Engine(#[from] nexora_core::NexoraError),
    #[error("corpus policy: {0}")]
    Policy(#[from] nexora_governance::GovernanceError),
}
