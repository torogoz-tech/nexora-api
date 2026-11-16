#[derive(Debug, Clone, PartialEq)]
pub struct RagCandidate {
    pub id: String,
    pub exact_score: f32,
    pub lexical_score: f32,
    pub semantic_score: f32,
    pub graph_score: f32,
    pub score: f32,
}

impl RagCandidate {
    pub fn new(
        id: &str,
        exact_score: f32,
        lexical_score: f32,
        semantic_score: f32,
        graph_score: f32,
    ) -> Self {
        Self {
            id: id.to_owned(),
            exact_score,
            lexical_score,
            semantic_score,
            graph_score,
            score: 0.0,
        }
    }
}
