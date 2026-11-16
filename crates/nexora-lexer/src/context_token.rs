pub(crate) struct ContextToken {
    pub kind: String,
    pub span: nexora_core::SourceSpan,
}
impl ContextToken {
    pub fn context<'a>(&'a self, source: &'a str) -> crate::token_context::TokenContext<'a> {
        crate::token_context::TokenContext {
            kind: &self.kind,
            text: &source[self.span.start.offset..self.span.end.offset],
        }
    }
}
