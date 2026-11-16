use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionResult {
    pub schema_version: u32,
    pub analysis_engine: String,
    pub configuration_fingerprint: String,
    pub source: nexora_core::SourceFile,
    pub units: Vec<crate::SyntaxUnit>,
    pub interpretations: Vec<crate::RegionInterpretation>,
    pub graph: nexora_graph::CodeGraph,
    /// Syntax coverage only; reference certainty is recorded on individual graph edges.
    pub complete: bool,
}

impl CompositionResult {
    pub(crate) fn into_analysis(
        mut self,
    ) -> Result<crate::AnalysisResult, nexora_core::NexoraError> {
        if self.units.is_empty() {
            return Err(nexora_core::NexoraError::Configuration(
                "composition has no host unit".into(),
            ));
        }
        let root = self.units.remove(0);
        let mut diagnostics = root.diagnostics;
        diagnostics.extend(
            self.units
                .iter()
                .flat_map(|unit| unit.diagnostics.iter().cloned()),
        );
        Ok(crate::AnalysisResult {
            source: self.source,
            tokens: root.tokens,
            cst: root.cst,
            ast: root.ast,
            symbols: root.symbols,
            graph: self.graph,
            diagnostics,
            embedded_units: self.units,
            interpretations: self.interpretations,
        })
    }
}
