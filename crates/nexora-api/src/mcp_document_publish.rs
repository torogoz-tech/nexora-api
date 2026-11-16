use nexora_platform::ProjectDocumentPublish;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpDocumentPublish {
    pub project_id: String,
    pub document_id: String,
    pub request_id: String,
    pub publication: ProjectDocumentPublish,
}
