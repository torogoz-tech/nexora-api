use nexora_platform::DocumentLinkRemove;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpDocumentLinkRemove {
    pub project_id: String,
    pub document_id: String,
    pub link_id: String,
    pub request_id: String,
    pub removal: DocumentLinkRemove,
}
