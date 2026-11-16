use nexora_platform::RepositoryRegistration;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpRepositoryRegistration {
    pub project_id: String,
    pub registration: RepositoryRegistration,
    pub request_id: String,
}
