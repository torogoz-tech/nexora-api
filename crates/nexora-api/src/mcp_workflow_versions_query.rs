use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpWorkflowVersionsQuery {
    pub workflow_id: String,
    pub before: Option<i64>,
    pub limit: Option<u16>,
}
