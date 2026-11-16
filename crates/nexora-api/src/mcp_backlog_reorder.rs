use nexora_platform::BacklogReorder;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpBacklogReorder {
    pub project_id: String,
    pub backlog_id: String,
    pub request_id: String,
    pub order: BacklogReorder,
}
