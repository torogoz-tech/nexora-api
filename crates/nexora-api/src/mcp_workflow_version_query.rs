use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpWorkflowVersionQuery {
    pub workflow_id: String,
    pub version_id: String,
}
