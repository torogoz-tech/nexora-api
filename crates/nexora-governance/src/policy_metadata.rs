use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyMetadata {
    pub id: String,
    pub name: String,
    pub version: crate::PolicyVersion,
    pub description: String,
    pub severity: crate::PolicySeverity,
    pub category: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}
impl PolicyMetadata {
    pub fn qualified_id(&self) -> String {
        self.package
            .as_ref()
            .map_or_else(|| self.id.clone(), |p| format!("{p}/{}", self.id))
    }
}
