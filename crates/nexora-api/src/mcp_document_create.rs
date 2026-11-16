use nexora_platform::ProjectDocumentCreate;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpDocumentCreate {
    pub project_id: String,
    pub request_id: String,
    pub definition: ProjectDocumentCreate,
}
