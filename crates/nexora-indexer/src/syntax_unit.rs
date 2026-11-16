use nexora_core::{AnalysisDiagnostic, LanguageId, RegionStatus, SourceSpan};
use serde::{Deserialize, Serialize};
/// All node and token IDs are local to this unit; qualify graph queries by unit ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxUnit {
    pub id: String,
    pub language: LanguageId,
    pub rule_version: String,
    pub profile_fingerprint: String,
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lexical_context: Option<String>,
    pub span: SourceSpan,
    pub status: RegionStatus,
    #[serde(default)]
    pub mutation_analysis: bool,
    #[serde(default)]
    pub direct_calls: bool,
    pub tokens: Vec<nexora_lexer::Token>,
    pub cst: nexora_parser::ConcreteSyntaxTree,
    pub ast: nexora_ast::AbstractSyntaxTree,
    pub symbols: nexora_symbols::SymbolTable,
    pub graph: nexora_graph::CodeGraph,
    pub diagnostics: Vec<AnalysisDiagnostic>,
}
