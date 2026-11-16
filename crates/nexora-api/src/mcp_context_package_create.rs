use nexora_platform::ProjectContextPackageCreate;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpContextPackageCreate {
    pub project_id: String,
    pub request_id: String,
    pub selection: ProjectContextPackageCreate,
}
