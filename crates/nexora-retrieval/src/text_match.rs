#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMatch {
    pub path: String,
    pub line: usize,
    pub text: String,
}
