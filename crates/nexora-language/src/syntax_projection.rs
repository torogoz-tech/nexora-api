use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyntaxProjection {
    pub ast_kind: String,
    pub role: nexora_core::SyntaxRole,
    pub name_capture: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<nexora_core::ScopePolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<crate::RegionProjection>,
}
