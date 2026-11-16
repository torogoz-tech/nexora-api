use nexora_platform::DeveloperWorkflowEdit;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpWorkflowChange {
    pub workflow_id: String,
    pub request_id: String,
    pub change: DeveloperWorkflowEdit,
}
