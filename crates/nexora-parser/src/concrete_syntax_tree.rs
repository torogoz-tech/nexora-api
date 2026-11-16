use crate::{ConcreteSyntaxNode, ParseDiagnostic};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConcreteSyntaxTree {
    pub nodes: Vec<ConcreteSyntaxNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default)]
    pub hierarchical: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_scope: Option<nexora_core::ScopePolicy>,
    pub diagnostics: Vec<ParseDiagnostic>,
}
