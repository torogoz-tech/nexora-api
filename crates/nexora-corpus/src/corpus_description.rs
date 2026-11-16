use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusDescription {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub fingerprint: String,
    pub cases: Vec<crate::CaseDescriptor>,
}
