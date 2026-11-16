use nexora_ast::AbstractSyntaxTree;
use nexora_core::{AnalysisDiagnostic, SourceFile};
use nexora_graph::CodeGraph;
use nexora_lexer::Token;
use nexora_parser::ConcreteSyntaxTree;
use nexora_symbols::SymbolTable;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub source: SourceFile,
    pub tokens: Vec<Token>,
    pub cst: ConcreteSyntaxTree,
    pub ast: AbstractSyntaxTree,
    pub symbols: SymbolTable,
    pub graph: CodeGraph,
    pub diagnostics: Vec<AnalysisDiagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedded_units: Vec<crate::SyntaxUnit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interpretations: Vec<crate::RegionInterpretation>,
}
