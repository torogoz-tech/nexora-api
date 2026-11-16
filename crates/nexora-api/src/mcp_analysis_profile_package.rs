use nexora_language::LanguageDefinition;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpAnalysisProfilePackage {
    pub key: String,
    pub version: String,
    pub name: String,
    pub language_id: String,
    pub models: Vec<LanguageDefinition>,
}
