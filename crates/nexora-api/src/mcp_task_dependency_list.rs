use nexora_platform::TaskDependencyQuery;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskDependencyList {
    #[serde(default)]
    pub project_id: Option<String>,
    pub task_id: String,
    #[serde(default)]
    pub query: TaskDependencyQuery,
}
