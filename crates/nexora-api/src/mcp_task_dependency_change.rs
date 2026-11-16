use nexora_platform::TaskDependencyChange;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskDependencyChange {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    pub dependency_id: String,
    pub request_id: String,
    pub change: TaskDependencyChange,
}
