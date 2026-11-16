use nexora_platform::TaskTemplatePublish;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskTemplatePublish {
    pub project_id: String,
    pub template_id: String,
    pub request_id: String,
    pub publication: TaskTemplatePublish,
}
