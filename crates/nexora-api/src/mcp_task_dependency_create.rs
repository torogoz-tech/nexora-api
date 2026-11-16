use nexora_platform::TaskDependencyCreate;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskDependencyCreate {
    #[serde(default)]
    pub project_id: Option<String>,
    pub request_id: String,
    pub dependency: TaskDependencyCreate,
}
