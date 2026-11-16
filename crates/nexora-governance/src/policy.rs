use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Policy {
    pub api_version: String,
    pub kind: String,
    pub metadata: crate::PolicyMetadata,
    pub spec: crate::PolicySpec,
}
