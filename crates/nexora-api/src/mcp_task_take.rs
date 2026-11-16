use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskTake {
    #[serde(default)]
    pub project_id: Option<String>,
    pub task_id: String,
    pub request_id: String,
    pub take: nexora_platform::TaskCodeTake,
}
