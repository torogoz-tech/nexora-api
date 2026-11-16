use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskTypeChange {
    pub project_id: String,
    pub type_code: String,
    pub request_id: String,
    pub change: nexora_platform::TaskTypeChange,
}
