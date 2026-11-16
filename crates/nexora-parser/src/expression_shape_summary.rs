#[derive(Default)]
pub(crate) struct ExpressionShapeSummary {
    pub operator: Option<u8>,
    pub descendant_kinds: u16,
    pub descendant_operators: [u64; 4],
}
