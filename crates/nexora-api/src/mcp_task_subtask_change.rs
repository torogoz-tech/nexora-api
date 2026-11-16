use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskSubtaskChange {
    pub task_id: String,
    pub child_task_id: String,
    pub request_id: String,
    pub change: nexora_platform::TaskSubtaskChange,
}
