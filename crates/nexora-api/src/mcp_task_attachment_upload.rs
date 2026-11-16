use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskAttachmentUpload {
    pub task_id: String,
    pub name: String,
    pub data_base64: String,
    pub request_id: String,
}
