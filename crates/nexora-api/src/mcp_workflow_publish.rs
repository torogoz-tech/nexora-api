use nexora_platform::WorkflowPublish;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpWorkflowPublish {
    pub workflow_id: String,
    pub request_id: String,
    pub publish: WorkflowPublish,
}
