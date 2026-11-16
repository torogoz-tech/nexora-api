use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyReference {
    pub id: String,
    pub version: crate::PolicyVersion,
}
