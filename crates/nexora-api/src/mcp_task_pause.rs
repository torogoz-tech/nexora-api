use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskPause {
    pub task_id: String,
    pub request_id: String,
    pub expected_revision: i64,
}
