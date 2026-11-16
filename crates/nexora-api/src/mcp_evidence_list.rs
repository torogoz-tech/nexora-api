use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpEvidenceList {
    pub project_id: String,
    pub repository_id: String,
    pub after: Option<String>,
    pub limit: Option<u16>,
}
