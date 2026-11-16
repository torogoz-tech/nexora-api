use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskDocumentsList {
    pub project_id: String,
    pub task_id: String,
    pub after: Option<String>,
    pub limit: Option<u16>,
    pub include_removed: Option<bool>,
}
