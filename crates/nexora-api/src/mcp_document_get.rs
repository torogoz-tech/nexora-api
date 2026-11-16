use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpDocumentGet {
    pub project_id: String,
    pub document_id: String,
}
