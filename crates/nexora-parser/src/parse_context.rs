#[derive(Debug, Default)]
pub struct ParseContext {
    pub cursor: usize,
    pub max_nesting: usize,
}
