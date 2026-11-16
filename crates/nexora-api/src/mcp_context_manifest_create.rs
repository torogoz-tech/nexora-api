use nexora_platform::ProjectContextManifestCreate;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpContextManifestCreate {
    pub project_id: String,
    pub request_id: String,
    pub selection: ProjectContextManifestCreate,
}
