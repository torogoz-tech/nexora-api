use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpDocumentApprovalGet {
    pub project_id: String,
    pub document_id: String,
    pub version_id: String,
    pub approval_id: String,
}
