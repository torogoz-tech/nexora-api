use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpDocumentHistory {
    pub project_id: String,
    pub document_id: String,
    pub after: Option<i64>,
    pub limit: Option<u16>,
}
