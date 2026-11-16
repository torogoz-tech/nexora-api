use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpApprovalRequestGet {
    pub project_id: String,
    pub approval_request_id: String,
}
