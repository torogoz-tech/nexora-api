use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskCommentsList {
    #[serde(default)]
    pub project_id: Option<String>,
    pub task_id: String,
    pub after: Option<String>,
    pub limit: Option<u16>,
}
