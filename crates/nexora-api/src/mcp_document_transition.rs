use nexora_platform::DocumentTransitionRequest;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpDocumentTransition {
    pub project_id: String,
    pub document_id: String,
    pub version_id: String,
    pub transition_id: String,
    pub request_id: String,
    pub transition: DocumentTransitionRequest,
}
