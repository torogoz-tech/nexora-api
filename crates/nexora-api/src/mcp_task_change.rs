use nexora_platform::TaskChange;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskChange {
    #[serde(default)]
    pub project_id: Option<String>,
    pub task_id: String,
    pub request_id: String,
    pub change: TaskChange,
}
