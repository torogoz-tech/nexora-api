use nexora_core::LanguageId;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageManifest {
    pub schema_version: u32,
    pub id: LanguageId,
    pub version: String,
    pub extensions: Vec<String>,
    #[serde(default)]
    pub resolution: Option<crate::ResolutionDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition: Option<crate::CompositionDefinition>,
}
