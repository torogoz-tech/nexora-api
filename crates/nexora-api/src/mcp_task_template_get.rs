use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskTemplateGet {
    pub project_id: String,
    pub template_id: String,
}
