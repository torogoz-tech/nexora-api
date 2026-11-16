use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpApprovalRequestCancel {
    pub project_id: String,
    pub approval_request_id: String,
    pub request_id: String,
    pub cancellation: nexora_platform::ApprovalRequestCancel,
}
