use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskTemplateVersionGet {
    pub project_id: String,
    pub template_id: String,
    pub version_id: String,
}
