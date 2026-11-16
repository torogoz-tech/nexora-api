use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusScores {
    pub true_positives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
    pub precision: Option<f64>,
    pub recall: Option<f64>,
}
impl CorpusScores {
    pub fn new(tp: usize, fp: usize, missing: usize) -> Self {
        Self {
            true_positives: tp,
            false_positives: fp,
            false_negatives: missing,
            precision: if tp + fp == 0 {
                None
            } else {
                Some(tp as f64 / (tp + fp) as f64)
            },
            recall: if tp + missing == 0 {
                None
            } else {
                Some(tp as f64 / (tp + missing) as f64)
            },
        }
    }
}
