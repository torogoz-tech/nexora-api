use nexora_platform::TaskTemplateApply;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskTemplateApply {
    pub project_id: String,
    pub request_id: String,
    pub application: TaskTemplateApply,
}
