use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskRelease {
    pub project_id: String,
    pub release_id: Option<String>,
    pub request_id: Option<String>,
    pub input: Option<serde_json::Value>,
}
