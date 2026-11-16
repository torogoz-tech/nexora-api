use nexora_platform::TaskTemplateChange;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskTemplateChange {
    pub project_id: String,
    pub template_id: String,
    pub request_id: String,
    pub change: TaskTemplateChange,
}
