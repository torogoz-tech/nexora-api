/// Grammar operations actually performed, including failed alternatives and recovery.
pub struct MeasuredParse {
    pub tree: crate::ConcreteSyntaxTree,
    pub steps: usize,
    pub budget_exhausted: bool,
}
