use nexora_core::ExpressionKind;
pub(crate) struct ExpressionTerm {
    pub kind: ExpressionKind,
    pub operator: Option<String>,
    pub start: usize,
    pub end: usize,
    pub operands: Vec<usize>,
    pub height: usize,
}
