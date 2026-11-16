use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpApprovalRequestsList {
    pub project_id: String,
    pub after: Option<String>,
    pub limit: Option<u16>,
    pub status: Option<nexora_platform::ApprovalRequestStatus>,
}
