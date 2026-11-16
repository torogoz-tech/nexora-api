use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusManifest {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub packs: Vec<crate::CorpusPack>,
    pub features: Vec<crate::CorpusFeature>,
    pub cases: Vec<String>,
}
