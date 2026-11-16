use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpDocumentVersionsList {
    pub project_id: String,
    pub document_id: String,
    pub before: Option<i64>,
    pub limit: Option<u16>,
}
