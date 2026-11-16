use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PolicyFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_matches: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_matches: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherits: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implements: Option<String>,
}
