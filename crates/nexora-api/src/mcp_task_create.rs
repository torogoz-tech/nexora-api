use nexora_platform::TaskCreate;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskCreate {
    #[serde(default)]
    pub project_id: Option<String>,
    pub request_id: String,
    pub definition: TaskCreate,
}
