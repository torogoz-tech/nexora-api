#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskProjectChange {
    pub task_id: String,
    pub request_id: String,
    pub change: nexora_platform::TaskProjectChange,
}
