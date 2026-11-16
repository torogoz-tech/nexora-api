use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpContextPackageGet {
    pub project_id: String,
    pub package_id: String,
}
