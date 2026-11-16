use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpContextPackageExport {
    pub project_id: String,
    pub package_id: String,
    pub format: String,
}
