use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseDescriptor {
    pub id: String,
    pub pack: String,
    pub feature: String,
    pub classification: crate::CorpusClassification,
    pub benchmark: bool,
    pub replicas: usize,
    pub fingerprint: String,
}
