use nexora_core::SourceSpan;
pub struct NodeAnchor {
    pub path: Option<String>,
    pub span: Option<SourceSpan>,
    pub verified: bool,
}
