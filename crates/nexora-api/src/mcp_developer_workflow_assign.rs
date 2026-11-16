use nexora_platform::DeveloperWorkflowChange;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpDeveloperWorkflowAssign {
    pub actor_id: String,
    pub request_id: String,
    pub assignment: DeveloperWorkflowChange,
}
