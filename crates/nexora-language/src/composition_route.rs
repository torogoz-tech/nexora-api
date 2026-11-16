use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositionRoute {
    pub language: nexora_core::LanguageId,
    pub context: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lexical_context: Option<String>,
    pub scope: nexora_core::ScopePolicy,
}
