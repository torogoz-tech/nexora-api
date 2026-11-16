use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpContextManifestStatus {
    pub project_id: String,
    pub manifest_id: String,
}
