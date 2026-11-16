use nexora_platform::DeveloperWorkflowCreate;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpWorkflowCreate {
    pub request_id: String,
    pub definition: DeveloperWorkflowCreate,
}
