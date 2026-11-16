use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpProjectChange {
    pub project_id: String,
    pub request_id: String,
    pub change: nexora_platform::ProjectChange,
}
