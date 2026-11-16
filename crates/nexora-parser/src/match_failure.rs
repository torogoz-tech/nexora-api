#[derive(Debug, Clone)]
pub struct MatchFailure {
    pub at: usize,
    pub message: String,
    pub fatal: bool,
}
