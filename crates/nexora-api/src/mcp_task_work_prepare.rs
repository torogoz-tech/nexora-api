use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskWorkPrepare {
    pub project_id: String,
    pub task_id: String,
    pub request_id: String,
    pub preparation: nexora_platform::TaskWorkPrepare,
}
