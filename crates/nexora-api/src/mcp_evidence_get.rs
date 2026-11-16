use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpEvidenceGet {
    pub project_id: String,
    pub evidence_id: String,
    pub include_content: Option<bool>,
}
