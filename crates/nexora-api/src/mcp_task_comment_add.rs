use nexora_platform::TaskCommentCreate;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskCommentAdd {
    #[serde(default)]
    pub project_id: Option<String>,
    pub task_id: String,
    pub request_id: String,
    pub comment: TaskCommentCreate,
}
