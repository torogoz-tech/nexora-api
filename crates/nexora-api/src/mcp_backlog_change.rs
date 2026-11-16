use nexora_platform::BacklogChange;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpBacklogChange {
    pub project_id: String,
    pub backlog_id: String,
    pub request_id: String,
    pub change: BacklogChange,
}
