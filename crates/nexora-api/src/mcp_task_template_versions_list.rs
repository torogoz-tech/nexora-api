use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskTemplateVersionsList {
    pub project_id: String,
    pub template_id: String,
    pub before: Option<i64>,
    pub limit: Option<u16>,
}
