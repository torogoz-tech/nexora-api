pub(crate) enum IndexAnalysisPlan {
    Local(Box<crate::AnalysisPlan>),
    Composed(Box<crate::CompositionAnalyzer>),
}
impl IndexAnalysisPlan {
    pub fn resolve_modules(
        &self,
        graph: &mut nexora_graph::CodeGraph,
        manifest: &nexora_language::LanguageManifest,
        diagnostics: &mut Vec<nexora_core::AnalysisDiagnostic>,
    ) -> Result<(), nexora_core::NexoraError> {
        match self {
            Self::Local(plan) => crate::ModuleResolver::resolve_prepared(
                graph,
                manifest,
                diagnostics,
                plan.resolution.clone(),
            ),
            Self::Composed(analyzer) => analyzer.resolve_modules(graph, diagnostics),
        }
    }
}
