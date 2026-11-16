use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskWorkQuery {
    pub project_id: String,
    pub task_id: String,
    pub session_id: String,
}
