use nexora_platform::RepositoryJobRequest;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpRepositoryJobRequest {
    pub project_id: String,
    pub repository_id: String,
    pub request_id: String,
    pub request: RepositoryJobRequest,
}
