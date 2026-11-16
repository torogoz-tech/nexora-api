use crate::RagCandidate;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct RagFusion {
    exact_weight: f32,
    lexical_weight: f32,
    semantic_weight: f32,
    graph_weight: f32,
}

impl Default for RagFusion {
    fn default() -> Self {
        Self {
            exact_weight: 0.30,
            lexical_weight: 0.25,
            semantic_weight: 0.30,
            graph_weight: 0.15,
        }
    }
}

impl RagFusion {
    pub fn rank(
        &self,
        _query: &str,
        candidates: Vec<RagCandidate>,
        limit: usize,
    ) -> Vec<RagCandidate> {
        let mut merged = HashMap::new();
        for candidate in candidates {
            let entry = merged
                .entry(candidate.id.clone())
                .or_insert_with(|| RagCandidate::new(&candidate.id, 0.0, 0.0, 0.0, 0.0));
            entry.exact_score = entry.exact_score.max(candidate.exact_score);
            entry.lexical_score = entry.lexical_score.max(candidate.lexical_score);
            entry.semantic_score = entry.semantic_score.max(candidate.semantic_score);
            entry.graph_score = entry.graph_score.max(candidate.graph_score);
        }
        let mut ranked: Vec<_> = merged
            .into_values()
            .map(|mut candidate| {
                candidate.score = candidate.exact_score * self.exact_weight
                    + candidate.lexical_score * self.lexical_weight
                    + candidate.semantic_score * self.semantic_weight
                    + candidate.graph_score * self.graph_weight;
                candidate
            })
            .collect();
        ranked.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.id.cmp(&right.id))
        });
        ranked.truncate(limit);
        ranked
    }
}
