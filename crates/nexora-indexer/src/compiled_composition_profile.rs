pub(crate) struct CompiledCompositionProfile {
    pub language: nexora_language::LanguageDefinition,
    pub fingerprint: String,
    pub plan: crate::AnalysisPlan,
}
