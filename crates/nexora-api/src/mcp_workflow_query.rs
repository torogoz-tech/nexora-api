use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpWorkflowQuery {
    pub workflow_id: String,
}
