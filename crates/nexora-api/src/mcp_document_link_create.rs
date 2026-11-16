use nexora_platform::DocumentLinkCreate;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpDocumentLinkCreate {
    pub project_id: String,
    pub document_id: String,
    pub request_id: String,
    pub link: DocumentLinkCreate,
}
