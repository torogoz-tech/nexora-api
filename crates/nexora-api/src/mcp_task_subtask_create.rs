use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskSubtaskCreate {
    pub task_id: String,
    pub request_id: String,
    pub subtask: nexora_platform::TaskSubtaskCreate,
}
