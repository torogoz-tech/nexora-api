use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskApprovalGet {
    #[serde(default)]
    pub project_id: Option<String>,
    pub task_id: String,
    pub approval_id: String,
}
