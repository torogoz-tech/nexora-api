pub(crate) struct CompositionTask {
    pub id: String,
    pub language: String,
    pub context: Option<String>,
    pub lexical_context: Option<String>,
    pub span: nexora_core::SourceSpan,
    pub scope: Option<nexora_core::ScopePolicy>,
}
