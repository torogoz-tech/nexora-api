use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskCommit {
    pub task_id: String,
    pub commit_id: Option<String>,
    pub request_id: Option<String>,
    pub input: Option<serde_json::Value>,
}
