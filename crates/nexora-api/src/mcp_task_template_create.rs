use nexora_platform::TaskTemplateCreate;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskTemplateCreate {
    pub project_id: String,
    pub request_id: String,
    pub definition: TaskTemplateCreate,
}
