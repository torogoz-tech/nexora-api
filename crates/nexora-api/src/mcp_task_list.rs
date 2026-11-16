use nexora_platform::TaskListQuery;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskList {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub query: TaskListQuery,
}
