use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskWorkLog {
    pub task_id: String,
    pub request_id: Option<String>,
    pub work_log_id: Option<String>,
    pub input: Option<nexora_platform::TaskWorkLogInput>,
}
