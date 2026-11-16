use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskAttachmentsQuery {
    pub task_id: String,
    pub after: Option<String>,
}
