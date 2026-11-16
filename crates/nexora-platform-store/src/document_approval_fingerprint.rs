use crate::project_context_checksum::ProjectContextChecksum;
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct DocumentApprovalFingerprint;
impl DocumentApprovalFingerprint {
    pub fn parameters(input: &DocumentTransitionRequest) -> Result<Value, PlatformError> {
        let mut value = input.clone();
        value.approval_id = None;
        serde_json::to_value(value).map_err(|_| PlatformError::Storage)
    }
    pub fn checksum(
        root: &ProjectDocument,
        version: &ProjectDocumentVersion,
        edge: &WorkflowTransition,
        input: &DocumentTransitionRequest,
    ) -> Result<String, PlatformError> {
        ProjectContextChecksum::value(
            json!({"project_id":root.project_id,"document_id":root.id,"document_revision":root.revision,"version_id":version.id,"state_revision":version.state_revision,"content_checksum":version.checksum,"workflow_version_id":version.workflow_version_id,"transition":edge,"parameters":Self::parameters(input)?}),
        )
    }
}
