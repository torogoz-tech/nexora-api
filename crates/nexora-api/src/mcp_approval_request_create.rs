use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpApprovalRequestCreate {
    pub project_id: String,
    pub request_id: String,
    pub definition: nexora_platform::ApprovalRequestCreate,
}
