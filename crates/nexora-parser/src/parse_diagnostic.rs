use nexora_core::SourceSpan;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseDiagnostic {
    pub message: String,
    pub span: SourceSpan,
}
