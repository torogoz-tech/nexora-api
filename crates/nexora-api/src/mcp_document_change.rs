use nexora_platform::ProjectDocumentChange;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpDocumentChange {
    pub project_id: String,
    pub document_id: String,
    pub request_id: String,
    pub change: ProjectDocumentChange,
}
