use nexora_core::{AnalysisDiagnostic, NexoraError};
use nexora_graph::CodeGraph;
pub(crate) struct CachedFile {
    pub hash: String,
    pub graph: CodeGraph,
    pub diagnostics: Vec<AnalysisDiagnostic>,
    pub weight: usize,
}
impl CachedFile {
    pub fn new(
        hash: String,
        graph: CodeGraph,
        diagnostics: Vec<AnalysisDiagnostic>,
    ) -> Result<Self, NexoraError> {
        let weight = serde_json::to_vec(&(&hash, &graph, &diagnostics))
            .map_err(|e| NexoraError::Configuration(e.to_string()))?
            .len();
        Ok(Self {
            hash,
            graph,
            diagnostics,
            weight,
        })
    }
}
