use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpBacklogGet {
    pub project_id: String,
    pub backlog_id: String,
}
