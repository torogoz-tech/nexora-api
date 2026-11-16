use nexora_core::SourceSpan;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub kind: String,
    pub text: String,
    pub span: SourceSpan,
    pub trivia: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<nexora_core::LexicalRegion>,
}
