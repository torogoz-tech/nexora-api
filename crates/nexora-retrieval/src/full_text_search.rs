use crate::TextMatch;
pub trait FullTextSearch {
    fn search_code(&self, query: &str, limit: usize) -> Vec<TextMatch>;
}
