use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskWorkReport {
    pub project_id: String,
    pub task_id: String,
    pub session_id: String,
    pub request_id: String,
    pub report: nexora_platform::TaskWorkReport,
}
