use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusCase {
    pub id: String,
    pub pack: String,
    pub feature: String,
    pub classification: crate::CorpusClassification,
    pub mode: crate::CorpusMode,
    pub input: String,
    #[serde(default = "CorpusCase::one")]
    pub replicas: usize,
    #[serde(default)]
    pub benchmark: bool,
    #[serde(default)]
    pub policy: Option<String>,
    pub expected: crate::CorpusExpectation,
}
impl CorpusCase {
    fn one() -> usize {
        1
    }
}
