use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpDocumentPublicationsList {
    pub project_id: String,
    pub document_id: String,
    pub after: Option<String>,
    pub limit: Option<u16>,
}
