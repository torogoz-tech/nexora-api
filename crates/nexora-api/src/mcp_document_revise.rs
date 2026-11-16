use nexora_platform::ProjectDocumentRevise;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpDocumentRevise {
    pub project_id: String,
    pub document_id: String,
    pub request_id: String,
    pub revision: ProjectDocumentRevise,
}
